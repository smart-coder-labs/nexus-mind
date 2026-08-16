//! The robot.
//!
//! # It is decoration, and the code is shaped so it cannot become anything else
//!
//! Everything the mascot shows is already on screen as a counter, a gauge or
//! the pipeline diagram. Nothing here returns information; it returns pixels or
//! it returns nothing. A terminal that cannot draw it loses a picture and no
//! meaning, silently — see `README.md`, "The mascot".
//!
//! # Two ways of drawing, picked at startup
//!
//! **Graphics protocol** (kitty, iTerm2, sixel): the terminal is handed the
//! real image and draws real pixels. This is what the art actually looks like.
//! It is used only when the terminal *answers a query saying it supports it* —
//! never guessed from `TERM`, because sending sixel to a terminal that does not
//! understand it corrupts the screen, and a decoration may not risk the
//! interface it decorates.
//!
//! **Quadrants**: the fallback, and what most terminals get. A cell shows two
//! colours; the quadrant glyphs (`▘▝▖▗▚▞▌▐▛▜▙▟`, U+2596–U+259F) decide which
//! shape those two colours take, giving 2x2 pixels per cell. Every one is in
//! Block Elements, as widely supported as `▀` — unlike the sextants (U+1FB00),
//! which would buy 2x3 and demand a font most people do not have.
//!
//! Both paths draw the same animations from the same frame counts; only the
//! fidelity differs.

use ratatui::prelude::*;

/// Frame widths in *cells* for the quadrant path, smallest first. A frame is
/// `w` cells wide and `w / 2` tall, which is square on screen.
pub const SIZES: [usize; 2] = [24, 32];

/// Pixels per cell in the quadrant path: two across, two down.
const SUB_X: usize = 2;
const SUB_Y: usize = 2;

/// Alpha at or above this counts as ink; below it the panel shows through.
const INK: u8 = 90;

/// The glyph for a 2x2 mask, bit 0 = top-left, then top-right, bottom-left,
/// bottom-right.
pub const QUADRANTS: [&str; 16] = [
    " ", "▘", "▝", "▀", "▖", "▌", "▞", "▛", "▗", "▚", "▐", "▜", "▄", "▙", "▟", "█",
];

// ── Assets ───────────────────────────────────────────────────────────────────

/// One animation set at every fidelity it ships in.
#[derive(Clone, Copy)]
pub struct SetAssets {
    /// Raw RGBA for the quadrant path, one entry per [`SIZES`] entry.
    quad: [&'static [u8]; SIZES.len()],
    /// Length-prefixed PNGs for the graphics path: a little-endian `u32`
    /// before each frame. One file, no manifest to fall out of sync with.
    hi: &'static [u8],
}

#[cfg(feature = "mascot")]
macro_rules! set {
    ($name:literal) => {
        SetAssets {
            quad: [
                include_bytes!(concat!("../assets/mascot/c24/", $name, ".rgba")),
                include_bytes!(concat!("../assets/mascot/c32/", $name, ".rgba")),
            ],
            hi: include_bytes!(concat!("../assets/mascot/hi/", $name, ".frames")),
        }
    };
}

#[cfg(not(feature = "mascot"))]
macro_rules! set {
    ($name:literal) => {
        SetAssets {
            quad: [&[], &[]],
            hi: &[],
        }
    };
}

/// The sets the robot works through while something is running, in the order
/// they play. Every one of them is a complete cycle from the sheet.
pub const WORKING: [SetAssets; 8] = [
    set!("walk"),
    set!("scan"),
    set!("analyze"),
    set!("pick_up"),
    set!("carry"),
    set!("transfer"),
    set!("type"),
    set!("run"),
];

/// Sitting poses, for when nothing is happening.
pub const RESTING: SetAssets = set!("reposo");

/// The finish.
pub const CELEBRATING: SetAssets = set!("celebrate");

impl SetAssets {
    /// Frames in this set, counted from the smallest quadrant asset — the one
    /// with a fixed frame size, so the count needs no header.
    fn len(&self) -> usize {
        self.quad[0].len().checked_div(quad_frame_bytes(SIZES[0])).unwrap_or(0)
    }

    fn quad_frame(&self, size_index: usize, frame: usize, cells: usize) -> Option<&'static [u8]> {
        let per = quad_frame_bytes(cells);
        let bytes = self.quad.get(size_index)?;
        bytes.get(frame * per..(frame + 1) * per)
    }

    /// The `frame`-th PNG out of the length-prefixed blob.
    fn hi_frame(&self, frame: usize) -> Option<&'static [u8]> {
        let mut at = 0usize;
        for i in 0..=frame {
            let header = self.hi.get(at..at + 4)?;
            let len = u32::from_le_bytes(header.try_into().ok()?) as usize;
            at += 4;
            let body = self.hi.get(at..at + len)?;
            if i == frame {
                return Some(body);
            }
            at += len;
        }
        None
    }
}

const fn quad_frame_bytes(cells: usize) -> usize {
    (cells * SUB_X) * (cells / 2 * SUB_Y) * 4
}

// ── What to play ─────────────────────────────────────────────────────────────

/// What the robot is doing, in the only terms it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    /// Nothing is happening. Sitting poses, slowly.
    Resting,
    /// Something is running. Works through every action set in turn, each one
    /// a complete cycle, then starts again.
    Working,
    /// Something finished cleanly. Plays once and holds.
    Celebrating,
}

impl Mood {
    /// Milliseconds a frame is held.
    ///
    /// The working cycle is the fast one: it is meant to read as motion, and a
    /// scan that takes minutes is a long time to watch something dawdle. The
    /// resting poses are not a cycle at all — they are eight separate sitting
    /// poses — so they are held long enough to read as *poses* rather than as a
    /// broken animation.
    fn frame_ms(self) -> u64 {
        match self {
            // Slow enough to read as movement rather than a flicker. The
            // working cycle is the one on screen for minutes at a time, and at
            // 130ms it pulled the eye off the counters beside it.
            Mood::Working => 200,
            Mood::Celebrating => 220,
            Mood::Resting => 1200,
        }
    }

    /// The set and frame to show at this moment.
    fn pick(self, elapsed_ms: u64) -> Option<(SetAssets, usize)> {
        let step = (elapsed_ms / self.frame_ms()) as usize;
        match self {
            Mood::Resting => {
                let n = RESTING.len();
                (n > 0).then(|| (RESTING, step % n))
            }
            Mood::Celebrating => {
                let n = CELEBRATING.len();
                // Holds on the last frame: a finished run does not quietly
                // start celebrating again and suggest the work resumed.
                (n > 0).then(|| (CELEBRATING, step.min(n - 1)))
            }
            Mood::Working => {
                // One flat playlist across every set, so each animation runs to
                // its end before the next begins and the whole thing loops.
                let total: usize = WORKING.iter().map(|s| s.len()).sum();
                if total == 0 {
                    return None;
                }
                let mut at = step % total;
                for set in WORKING {
                    let n = set.len();
                    if at < n {
                        return Some((set, at));
                    }
                    at -= n;
                }
                None
            }
        }
    }
}

// ── The frame ────────────────────────────────────────────────────────────────

/// One frame, ready to draw either way.
pub struct Mascot {
    set: SetAssets,
    frame: usize,
    /// Cells across for the quadrant path; the graphics path uses the box it
    /// is given.
    cells: usize,
}

impl Mascot {
    /// The largest quadrant frame that fits in `w` x `h` cells.
    fn best_size(w: u16, h: u16) -> Option<usize> {
        SIZES
            .iter()
            .rev()
            .copied()
            .find(|&cells| w as usize >= cells && h as usize >= cells / 2)
    }

    /// What to draw for this mood at this moment in a box of `w` x `h` cells,
    /// or `None` when there is nothing to draw: no assets, or no room.
    ///
    /// `elapsed_ms` drives the animation rather than a frame counter so the
    /// speed is the same whatever the draw loop is doing.
    pub fn for_state(mood: Mood, elapsed_ms: u64, w: u16, h: u16) -> Option<Self> {
        let cells = Self::best_size(w, h)?;
        let (set, frame) = mood.pick(elapsed_ms)?;
        Some(Self { set, frame, cells })
    }

    /// The box this frame will fill, in cells.
    pub fn cells(&self) -> (u16, u16) {
        (self.cells as u16, (self.cells / 2) as u16)
    }

    /// The PNG for the graphics path.
    pub fn image_bytes(&self) -> Option<&'static [u8]> {
        self.set.hi_frame(self.frame)
    }

    /// Whether a mascot can be drawn at all in this build.
    pub fn compiled_in() -> bool {
        !RESTING.hi.is_empty()
    }

    fn pixel(&self, x: usize, y: usize) -> Option<Rgb> {
        let index = SIZES.iter().position(|&s| s == self.cells)?;
        let bytes = self.set.quad_frame(index, self.frame, self.cells)?;
        let i = (y * self.cells * SUB_X + x) * 4;
        let (r, g, b, a) = (*bytes.get(i)?, bytes[i + 1], bytes[i + 2], bytes[i + 3]);
        (a >= INK).then_some(Rgb(r, g, b))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Rgb(u8, u8, u8);

impl Rgb {
    fn luma(self) -> u32 {
        299 * self.0 as u32 + 587 * self.1 as u32 + 114 * self.2 as u32
    }
    fn color(self) -> Color {
        Color::Rgb(self.0, self.1, self.2)
    }
}

impl Widget for Mascot {
    /// The quadrant path: four pixels per cell, two colours per cell, and the
    /// glyph carries the shape.
    ///
    /// Transparency is handled by *not writing* rather than by painting a
    /// background: a cell with no ink is left untouched so the panel behind
    /// shows through. That is what lets this sit inside another widget instead
    /// of on top of a rectangle.
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (want_w, want_h) = self.cells();
        let cols = area.width.min(want_w) as usize;
        let rows = area.height.min(want_h) as usize;
        let x_off = (area.width as usize) - cols;
        let y_off = (area.height as usize) - rows;

        for row in 0..rows {
            for col in 0..cols {
                let mut sub = [None; SUB_X * SUB_Y];
                for dy in 0..SUB_Y {
                    for dx in 0..SUB_X {
                        sub[dy * SUB_X + dx] = self.pixel(col * SUB_X + dx, row * SUB_Y + dy);
                    }
                }
                let Some((symbol, fg, bg)) = resolve_cell(&sub) else {
                    continue;
                };
                let cell = &mut buf[(
                    area.x + (x_off + col) as u16,
                    area.y + (y_off + row) as u16,
                )];
                cell.set_symbol(symbol).set_fg(fg);
                cell.set_bg(bg.unwrap_or(Color::Reset));
            }
        }
    }
}

/// Turns four subpixels into a glyph and at most two colours.
///
/// Returns `None` for a cell with no ink at all — the caller must leave those
/// alone rather than paint them.
///
/// When every quarter has ink, the two colours are the lightest and darkest
/// present and each quarter joins whichever it is nearer: two colours is all a
/// cell has, and choosing the extremes keeps the contrast that carries the
/// shape. When only some quarters have ink, they share one colour and the rest
/// stay transparent — those are the silhouette's edge cells, where preserving
/// transparency matters more than a second shade.
fn resolve_cell(sub: &[Option<Rgb>; SUB_X * SUB_Y]) -> Option<(&'static str, Color, Option<Color>)> {
    let inked = sub.iter().filter(|p| p.is_some()).count();
    if inked == 0 {
        return None;
    }
    let lightest = sub.iter().flatten().copied().max_by_key(|c| c.luma())?;
    let darkest = sub.iter().flatten().copied().min_by_key(|c| c.luma())?;

    if inked < sub.len() {
        let mask = sub
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_some())
            .fold(0usize, |m, (i, _)| m | (1 << i));
        return Some((QUADRANTS[mask], lightest.color(), None));
    }

    let mask = sub.iter().enumerate().fold(0usize, |m, (i, p)| {
        let c = p.expect("every quarter has ink here");
        if c.luma().abs_diff(lightest.luma()) <= c.luma().abs_diff(darkest.luma()) {
            m | (1 << i)
        } else {
            m
        }
    });
    Some((QUADRANTS[mask], lightest.color(), Some(darkest.color())))
}

// ── Capability ───────────────────────────────────────────────────────────────

/// Whether this terminal can render the quadrant fallback.
///
/// A capability check, never a `TERM` allowlist — those strings under-report
/// (`xterm-256color` in a truecolor emulator, which is what VS Code reports)
/// and over-report in equal measure.
pub fn terminal_supports() -> bool {
    truecolor_available() && !locale_rules_out_unicode()
}

/// Truecolor, from the variable that exists to announce it.
///
/// Required rather than merely preferred: the frames are full-colour, and a
/// 256-colour approximation of a white-and-cyan robot is not worth showing.
fn truecolor_available() -> bool {
    std::env::var("COLORTERM")
        .map(|v| {
            let v = v.to_ascii_lowercase();
            v.contains("truecolor") || v.contains("24bit")
        })
        .unwrap_or(false)
}

/// Whether the locale *says* this terminal cannot do UTF-8.
///
/// # Why absence is not a refusal
///
/// This began as "the locale must say UTF-8", which is the wrong way round and
/// cost the feature on the most ordinary setup there is: VS Code's terminal on
/// macOS sets no `LANG` at all, so a perfectly capable terminal was told it was
/// incapable and the mascot silently never appeared.
///
/// An unset locale is not evidence of anything. What we emit is UTF-8 either
/// way — Rust strings are — and the locale does not change how the terminal
/// decodes them. So this backs off only when a locale is present *and* names a
/// charset that is not UTF-8, which is real evidence and is rare.
fn locale_rules_out_unicode() -> bool {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find_map(|v| verdict_for_locale(&v))
        .unwrap_or(false)
}

/// What one locale string says about Unicode.
///
/// `Some(true)` — it names a charset that is not UTF-8: evidence, back off.
/// `Some(false)` — it says UTF-8: evidence, go ahead.
/// `None` — it says nothing either way (`C`, `POSIX`, empty); look at the next
/// variable, and if they all say nothing, go ahead.
fn verdict_for_locale(value: &str) -> Option<bool> {
    let value = value.to_ascii_uppercase();
    if value.is_empty() {
        return None;
    }
    if value.contains("UTF-8") || value.contains("UTF8") {
        return Some(false);
    }
    // `C` and `POSIX` name no charset at all, and those terminals are UTF-8 in
    // practice; only an explicit non-UTF-8 charset counts against us.
    match value.split_once('.') {
        Some((_, charset)) if !charset.is_empty() => Some(true),
        _ => None,
    }
}

/// The operator's answer, if they gave one.
///
/// `--no-mascot` and `NEXUSMIND_MIGRATE_MASCOT=0` only ever turn it *off*.
/// There is deliberately no way to force it on: the checks exist because
/// drawing anyway produces a corrupted panel.
pub fn disabled_by_operator() -> bool {
    if std::env::args().any(|a| a == "--no-mascot") {
        return true;
    }
    matches!(
        std::env::var("NEXUSMIND_MIGRATE_MASCOT").as_deref(),
        Ok("0") | Ok("off") | Ok("false") | Ok("no")
    )
}

/// Runs the detection child and reads its answer.
///
/// The timeout is the point: the child may hang holding the terminal, and this
/// is the only place that can end it.
fn ask_in_child() -> Option<(String, (u16, u16))> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe().ok()?;
    let mut child = Command::new(exe)
        .arg("--detect-graphics")
        // `ratatui-image`'s `iterm2_from_env` claims the iTerm2 protocol for
        // anything whose TERM_PROGRAM contains "vscode", without asking the
        // terminal — and that guess overrides the terminal's real answer.
        // VS Code ships image support switched off, so the app emitted inline
        // image escapes that were silently swallowed and the mascot vanished
        // entirely: worse than the blocky fallback it replaced.
        //
        // Hiding the variable from the child leaves only the answers the
        // terminal actually gives (the kitty query and DA1 for sixel). This is
        // a terminal-name check, which I would not otherwise write — it is here
        // to disarm someone else's.
        .env_remove(if std::env::var("TERM_PROGRAM")
            .map(|t| t.contains("vscode"))
            .unwrap_or(false)
        {
            "TERM_PROGRAM"
        } else {
            "NEXUSMIND_UNUSED_PLACEHOLDER"
        })
        // stdin and stdout are the terminal: the query needs both. Only the
        // answer is piped.
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    let mut answer = String::new();
    if let Some(mut err) = child.stderr.take() {
        let reader = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = err.read_to_string(&mut buf);
            buf
        });
        // A terminal that is going to answer does so immediately; the crate's
        // own patience is one second, and this waits a little longer than that
        // before taking the decision away from it.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
            }
        }
        answer = reader.join().unwrap_or_default();
    }

    let mut parts = answer.split_whitespace();
    let name = parts.next()?.to_string();
    let w: u16 = parts.next()?.parse().ok()?;
    let h: u16 = parts.next()?.parse().ok()?;
    Some((name, (w, h)))
}

/// Whether the operator forbade the graphics path specifically, keeping the
/// quadrant fallback.
pub fn graphics_disabled_by_operator() -> bool {
    std::env::args().any(|a| a == "--no-mascot-graphics")
        || matches!(
            std::env::var("NEXUSMIND_MIGRATE_GRAPHICS").as_deref(),
            Ok("0") | Ok("off") | Ok("false") | Ok("no")
        )
}

// ── The graphics path ────────────────────────────────────────────────────────

/// Holds the terminal's answer about what it can draw.
///
/// Built once at start-up, because the query talks to the terminal directly and
/// doing that mid-run would fight the alternate screen.
pub struct Graphics {
    picker: ratatui_image::picker::Picker,
}

impl Graphics {
    /// Asks the terminal what it supports, and keeps the answer only if it is
    /// better than the fallback.
    ///
    /// Returns `None` when the terminal says "half blocks" — our quadrants beat
    /// that — or when it says nothing at all. **Must be called before entering
    /// the alternate screen**: it writes a query and reads the reply, and doing
    /// that with the UI already up would leave the response on screen.
    ///
    /// Failure here is not an error condition. It is the ordinary case on most
    /// terminals, and it is silent.
    /// Asks the terminal what it supports, in a child process.
    ///
    /// # Why a child process
    ///
    /// `Picker::from_query_stdio` spawns a thread that reads stdin and, when
    /// the terminal does not answer, **returns on a timeout without stopping
    /// it**. That thread stays blocked in `read(stdin)` for the life of the
    /// process and swallows every keystroke: the UI draws and nothing responds
    /// to the keyboard. On any terminal without image support — which is most
    /// of them — calling it directly makes the application unusable.
    ///
    /// So the query runs in a copy of this binary invoked with
    /// `--detect-graphics`. It inherits the terminal on stdin and stdout, so
    /// the escape sequence reaches the terminal and the reply reaches the
    /// query; the answer comes back on stderr, which is a pipe. If it does not
    /// finish in time it is killed, and the leaked thread dies with it.
    ///
    /// Returns `None` when the terminal says "half blocks" — our quadrants beat
    /// that — or when it says nothing. **Must be called before the alternate
    /// screen goes up.** Failure here is the ordinary case, and it is silent.
    pub fn detect() -> Option<Self> {
        use ratatui_image::picker::ProtocolType;
        if graphics_disabled_by_operator() {
            return None;
        }
        let (proto, font) = ask_in_child()?;
        let protocol_type = match proto.as_str() {
            "kitty" => ProtocolType::Kitty,
            "iterm2" => ProtocolType::Iterm2,
            "sixel" => ProtocolType::Sixel,
            _ => return None,
        };
        let mut picker = ratatui_image::picker::Picker::from_fontsize(font);
        picker.set_protocol_type(protocol_type);
        Some(Self { picker })
    }

    /// The other half of [`Self::detect`]: what the child process does.
    ///
    /// Writes `protocol width height` to stderr and exits. Called from `main`
    /// before anything else when `--detect-graphics` is present.
    pub fn run_detection_child() -> ! {
        use ratatui_image::picker::ProtocolType;
        let code = match ratatui_image::picker::Picker::from_query_stdio() {
            Ok(picker) => {
                let name = match picker.protocol_type() {
                    ProtocolType::Kitty => "kitty",
                    ProtocolType::Iterm2 => "iterm2",
                    ProtocolType::Sixel => "sixel",
                    ProtocolType::Halfblocks => "halfblocks",
                };
                let (w, h) = picker.font_size();
                eprintln!("{name} {w} {h}");
                0
            }
            Err(_) => 1,
        };
        // `exit`, not a return: the query's reader thread is still blocked on
        // stdin and would keep this process alive.
        std::process::exit(code)
    }

    /// The box, in cells, that shows a square frame *square* — and centred,
    /// because it is exactly the size of the image.
    ///
    /// # Why this is not simply `w / 2`
    ///
    /// The quadrant path can assume a cell is twice as tall as it is wide, and
    /// it is close enough. The graphics path cannot: the terminal reports its
    /// real font size, and in WezTerm at 14pt with line-height 1.06 a cell is
    /// nearer 8x22 than 8x16. Fitting a square image into a box sized `w x w/2`
    /// left it letterboxed against the top of the box with dead space beneath,
    /// which is why the robot sat too high.
    ///
    /// Height comes out of the aspect the terminal actually reported.
    pub fn box_for(&self, cells_wide: u16) -> (u16, u16) {
        let (fw, fh) = self.picker.font_size();
        let height = if fh == 0 {
            cells_wide / 2
        } else {
            // pixels across = cells_wide * fw; the same in pixels down, turned
            // back into cells.
            ((cells_wide as u32 * fw as u32) / fh as u32) as u16
        };
        (cells_wide, height.max(1))
    }

    /// The protocol's name, for the help screen.
    pub fn name(&self) -> &'static str {
        use ratatui_image::picker::ProtocolType;
        match self.picker.protocol_type() {
            ProtocolType::Kitty => "kitty",
            ProtocolType::Iterm2 => "iterm2",
            ProtocolType::Sixel => "sixel",
            ProtocolType::Halfblocks => "halfblocks",
        }
    }

    /// Encodes one frame for this terminal, or `None` if anything about it
    /// fails — a decode error, an encode error, a size the protocol dislikes.
    ///
    /// Every failure lands here and returns `None`, which the caller treats
    /// exactly like "no room": nothing is drawn and nothing is said.
    pub fn protocol(
        &self,
        mascot: &Mascot,
        area: Rect,
    ) -> Option<ratatui_image::protocol::Protocol> {
        let png = mascot.image_bytes()?;
        let image = image::load_from_memory(png).ok()?;
        self.picker
            .new_protocol(image, area, ratatui_image::Resize::Fit(None))
            .ok()
    }
}

/// Prints every decision that leads to the mascot appearing or not, and stops.
///
/// The degradation is silent by design, which is right for someone who does not
/// care and useless for someone who does. This is the way out of that: one
/// command that names the reason instead of a round of guesses.
pub fn explain_and_exit() -> ! {
    println!("nexusmind-migrate — por qué se ve (o no) la mascota\n");

    let compiled = Mascot::compiled_in();
    println!(
        "  frames embebidos ....... {}",
        if compiled { "sí" } else { "NO (build sin la feature `mascot`)" }
    );

    let color = std::env::var("COLORTERM").unwrap_or_default();
    let truecolor = truecolor_available();
    println!(
        "  COLORTERM .............. {} -> {}",
        if color.is_empty() { "(sin definir)" } else { &color },
        if truecolor { "truecolor" } else { "NO truecolor — la mascota se apaga" }
    );

    for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
        let value = std::env::var(key).unwrap_or_default();
        let verdict = match verdict_for_locale(&value) {
            Some(true) => "declara un charset que no es UTF-8 — la mascota se apaga",
            Some(false) => "UTF-8",
            None => "no dice nada (no cuenta en contra)",
        };
        println!(
            "  {key:<22} {} -> {verdict}",
            if value.is_empty() { "(sin definir)" } else { &value }
        );
    }

    let off = disabled_by_operator();
    println!(
        "  apagada a mano ......... {}",
        if off { "sí (--no-mascot o NEXUSMIND_MIGRATE_MASCOT)" } else { "no" }
    );

    let on = compiled && terminal_supports() && !off;
    println!("\n  ¿se dibuja? ............ {}", if on { "SÍ" } else { "NO" });

    if on {
        print!("  consultando al terminal por un protocolo gráfico… ");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        match Graphics::detect() {
            Some(g) => println!("{}", g.name()),
            None if graphics_disabled_by_operator() => println!("desactivado a mano"),
            None => println!("ninguno — se dibuja con glifos de cuadrante"),
        }
    }
    println!();
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOODS: [Mood; 3] = [Mood::Resting, Mood::Working, Mood::Celebrating];

    fn all_sets() -> Vec<SetAssets> {
        let mut v = WORKING.to_vec();
        v.push(RESTING);
        v.push(CELEBRATING);
        v
    }

    /// Both fidelities must hold the same number of frames, or the animation
    /// changes length depending on what the terminal can do.
    #[test]
    fn every_set_agrees_on_its_frame_count_across_fidelities() {
        if !Mascot::compiled_in() {
            return;
        }
        for set in all_sets() {
            let from_small = set.len();
            assert!(from_small >= 4, "a set with {from_small} frames is too short");

            for (i, &cells) in SIZES.iter().enumerate() {
                let per = quad_frame_bytes(cells);
                assert!(
                    set.quad[i].len().is_multiple_of(per),
                    "the {cells}-cell asset is not a whole number of frames"
                );
                assert_eq!(set.quad[i].len() / per, from_small);
            }

            let mut hi = 0;
            while set.hi_frame(hi).is_some() {
                hi += 1;
            }
            assert_eq!(hi, from_small, "the image frames disagree with the pixels");
        }
    }

    /// Ten sets ship, and the working cycle uses eight of them: every action
    /// animation the sheet has.
    #[test]
    fn the_working_cycle_uses_every_action_set() {
        assert_eq!(WORKING.len(), 8);
        if !Mascot::compiled_in() {
            return;
        }
        let total: usize = WORKING.iter().map(|s| s.len()).sum();
        assert!(total >= 40, "only {total} frames across the working cycle");

        // Every set is reached, and each runs to its end before the next
        // begins — that is what "complete cycles" means.
        let ms = Mood::Working.frame_ms();
        let seen: Vec<usize> = (0..total)
            .map(|step| {
                let (set, frame) = Mood::Working.pick(step as u64 * ms).unwrap();
                (set.len() * 1000) + frame
            })
            .collect();
        assert_eq!(seen.len(), total);
        // Frame indices within a set must ascend from 0 without gaps.
        let mut expect = 0;
        for set in WORKING {
            for f in 0..set.len() {
                assert_eq!(seen[expect] % 1000, f, "out of order at {expect}");
                expect += 1;
            }
        }
    }

    #[test]
    fn the_working_cycle_loops() {
        if !Mascot::compiled_in() {
            return;
        }
        let total: usize = WORKING.iter().map(|s| s.len()).sum();
        let ms = Mood::Working.frame_ms();
        let first = Mood::Working.pick(0).unwrap();
        let round = Mood::Working.pick(total as u64 * ms).unwrap();
        assert_eq!(first.1, round.1);
    }

    #[test]
    fn celebrating_holds_on_its_last_frame() {
        if !Mascot::compiled_in() {
            return;
        }
        let (set, frame) = Mood::Celebrating.pick(10_000_000).unwrap();
        assert_eq!(frame, set.len() - 1);
    }

    #[test]
    fn resting_cycles_the_sitting_poses() {
        if !Mascot::compiled_in() {
            return;
        }
        let n = RESTING.len();
        assert_eq!(n, 8, "the idle sheet is eight poses");
        let ms = Mood::Resting.frame_ms();
        assert_eq!(Mood::Resting.pick(0).unwrap().1, 0);
        assert_eq!(Mood::Resting.pick(ms * n as u64).unwrap().1, 0, "it loops");
    }

    /// The reported symptom: the robot sat too high in WezTerm. The box was
    /// sized `w x w/2`, which assumes a cell is exactly twice as tall as it is
    /// wide; WezTerm reported nearer 8x22, so a square image was letterboxed
    /// against the top with dead space beneath it.
    #[test]
    fn the_graphics_box_is_square_on_screen_whatever_the_font() {
        for (fw, fh) in [(8u16, 16u16), (8, 22), (10, 20), (7, 15), (12, 25)] {
            let mut picker = ratatui_image::picker::Picker::from_fontsize((fw, fh));
            picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
            let g = Graphics { picker };
            let (w, h) = g.box_for(40);
            assert_eq!(w, 40);

            // The box must be about as tall in pixels as it is wide, within
            // one cell of rounding.
            let across = w as i32 * fw as i32;
            let down = h as i32 * fh as i32;
            assert!(
                (across - down).abs() <= fh as i32,
                "at {fw}x{fh} the box is {across}x{down} px, not square"
            );
        }
    }

    /// A degenerate font size must not divide by zero.
    #[test]
    fn a_nonsense_font_size_still_yields_a_box() {
        let mut picker = ratatui_image::picker::Picker::from_fontsize((8, 0));
        picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
        let (w, h) = Graphics { picker }.box_for(40);
        assert_eq!((w, h), (40, 20));
    }

    #[test]
    fn the_largest_frame_that_fits_is_the_one_used() {
        assert_eq!(Mascot::best_size(32, 16), Some(32));
        assert_eq!(Mascot::best_size(31, 16), Some(24), "too narrow for 32");
        assert_eq!(Mascot::best_size(32, 15), Some(24), "too short for 32");
        assert_eq!(Mascot::best_size(200, 90), Some(32), "and never larger");
        assert_eq!(Mascot::best_size(23, 12), None, "nothing fits");
        assert_eq!(Mascot::best_size(0, 0), None);
    }

    #[test]
    fn a_frame_offers_both_fidelities() {
        if !Mascot::compiled_in() {
            return;
        }
        let m = Mascot::for_state(Mood::Working, 0, 32, 16).unwrap();
        assert_eq!(m.cells(), (32, 16));
        let png = m.image_bytes().expect("the graphics path needs an image");
        assert_eq!(&png[1..4], b"PNG", "not a PNG: {:?}", &png[..8.min(png.len())]);
        // Scan the whole grid rather than probing coordinates: which pixels
        // carry the robot depends on the pose, and a fixed probe tests the
        // pose, not the plumbing.
        let inked = (0..32 * SUB_Y)
            .flat_map(|y| (0..32 * SUB_X).map(move |x| (x, y)))
            .filter(|&(x, y)| m.pixel(x, y).is_some())
            .count();
        assert!(inked > 100, "only {inked} pixels of the frame carry ink");
    }

    /// A frame index past the end must report nothing rather than read into
    /// the next set's bytes.
    #[test]
    fn asking_past_the_end_returns_nothing() {
        if !Mascot::compiled_in() {
            return;
        }
        let n = RESTING.len();
        assert!(RESTING.hi_frame(n).is_none());
        assert!(RESTING.quad_frame(0, n, SIZES[0]).is_none());
    }

    // ── The quadrant renderer ────────────────────────────────────────────

    #[test]
    fn the_quadrant_table_matches_its_bit_layout() {
        assert_eq!(QUADRANTS.len(), 1 << (SUB_X * SUB_Y));
        assert_eq!(QUADRANTS[0b0000], " ");
        assert_eq!(QUADRANTS[0b0001], "▘", "bit 0 is top-left");
        assert_eq!(QUADRANTS[0b0010], "▝", "bit 1 is top-right");
        assert_eq!(QUADRANTS[0b0011], "▀", "both top quarters");
        assert_eq!(QUADRANTS[0b1100], "▄", "both bottom quarters");
        assert_eq!(QUADRANTS[0b1111], "█");
        assert_eq!(QUADRANTS[0b1001], "▚", "top-left and bottom-right");
    }

    #[test]
    fn a_cell_with_no_ink_is_left_alone() {
        assert!(resolve_cell(&[None, None, None, None]).is_none());
    }

    #[test]
    fn a_partly_inked_cell_keeps_the_background_transparent() {
        let white = Some(Rgb(240, 240, 240));
        let (symbol, fg, bg) = resolve_cell(&[white, None, None, None]).unwrap();
        assert_eq!(symbol, "▘");
        assert_eq!(fg, Color::Rgb(240, 240, 240));
        assert_eq!(bg, None, "the other three quarters must show the panel");
    }

    #[test]
    fn a_full_cell_uses_the_lightest_and_darkest_it_contains() {
        let light = Rgb(250, 250, 250);
        let dark = Rgb(10, 10, 10);
        let (symbol, fg, bg) =
            resolve_cell(&[Some(light), Some(dark), Some(dark), Some(light)]).unwrap();
        assert_eq!(fg, light.color());
        assert_eq!(bg, Some(dark.color()));
        assert_eq!(symbol, "▚");
    }

    #[test]
    fn every_glyph_is_a_block_element() {
        for g in QUADRANTS {
            for ch in g.chars() {
                assert!(
                    ch == ' ' || ('\u{2580}'..='\u{259F}').contains(&ch),
                    "{ch:?} ({:04X}) is outside Block Elements",
                    ch as u32
                );
            }
        }
    }

    #[test]
    fn rendering_never_panics_however_little_room_it_is_given() {
        if !Mascot::compiled_in() {
            return;
        }
        for mood in MOODS {
            for (aw, ah) in [(0u16, 0u16), (1, 1), (3, 2), (32, 16)] {
                let Some(m) = Mascot::for_state(mood, 400, 32, 16) else {
                    continue;
                };
                let mut buf = Buffer::empty(Rect::new(0, 0, 100, 50));
                m.render(Rect::new(0, 0, aw, ah), &mut buf);
            }
        }
    }

    #[test]
    fn transparent_pixels_do_not_paint_over_the_panel() {
        if !Mascot::compiled_in() {
            return;
        }
        let mut buf = Buffer::empty(Rect::new(0, 0, 32, 16));
        for y in 0..16 {
            for x in 0..32 {
                buf[(x, y)].set_symbol("·");
            }
        }
        Mascot::for_state(Mood::Resting, 0, 32, 16)
            .unwrap()
            .render(Rect::new(0, 0, 32, 16), &mut buf);
        let untouched = (0..16)
            .flat_map(|y| (0..32).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() == "·")
            .count();
        assert!(untouched > 100, "only {untouched} of 512 cells survived");
    }

    #[test]
    fn something_is_actually_drawn() {
        if !Mascot::compiled_in() {
            return;
        }
        let mut buf = Buffer::empty(Rect::new(0, 0, 32, 16));
        Mascot::for_state(Mood::Celebrating, 0, 32, 16)
            .unwrap()
            .render(Rect::new(0, 0, 32, 16), &mut buf);
        let inked = (0..16)
            .flat_map(|y| (0..32).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() != " ")
            .count();
        assert!(inked > 40, "only {inked} cells carry the robot");
    }

    #[test]
    fn support_is_decided_by_capability_not_by_terminal_name() {
        let source = include_str!("mascot.rs");
        assert!(
            !source.contains("TERM_PROGRAM") || source.contains("fn name("),
            "the capability checks must not read TERM_PROGRAM"
        );
        let checker = source
            .split("fn truecolor_available")
            .nth(1)
            .expect("the function must exist");
        let body = &checker[..checker.find("\n}").unwrap_or(checker.len())];
        assert!(!body.contains("\"TERM\""), "must not read TERM");
        assert!(body.contains("COLORTERM"));
    }

    /// The reported failure: no robot in VS Code's terminal on macOS, which
    /// sets no `LANG`. An absent locale must not be read as "cannot do
    /// Unicode" — it is no evidence at all.
    #[test]
    fn an_absent_locale_does_not_rule_out_unicode() {
        // Locale detection is pure over the strings it is given, so the rule
        // can be checked without touching the process environment — which a
        // parallel test suite must never do.
        for (value, expected) in [
            ("en_US.UTF-8", Some(false)),
            ("C.UTF-8", Some(false)),
            ("en_US.utf8", Some(false)),
            ("C", None),
            ("POSIX", None),
            ("", None),
            ("en_US.ISO8859-1", Some(true)),
            ("ru_RU.KOI8-R", Some(true)),
        ] {
            assert_eq!(verdict_for_locale(value), expected, "{value:?}");
        }
        // And the whole point: variables that say nothing are not a refusal.
        let all_silent = ["", "C", "POSIX"]
            .iter()
            .find_map(|v| verdict_for_locale(v))
            .unwrap_or(false);
        assert!(!all_silent, "silence must not read as incapable");
    }
}
