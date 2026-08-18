# Robot mascot — asset contract (floating mascot v1)

The floating mascot that roams the landing consumes **individual frames per action**.
Drop the files here following this exact convention and I'll wire + tune the animation.

## Hard requirement #1: real alpha transparency
Each PNG must have a **truly transparent alpha channel** — NOT a painted checkerboard,
NOT a dark/blue background. If it has a baked background it will render as an ugly
rectangle floating over the page. This is the whole reason the first version failed.

## Folder layout
One folder per state, zero-padded frame files:

```
public/robot/frames/
├── idle/       idle-01.png, idle-02.png, ...        (front view, subtle)
├── walk/       walk-01.png, ...                     (SIDE profile, facing right)
├── carry/      carry-01.png, ...                    (SIDE, holding the data box)
├── scan/       scan-01.png, ...                     (magnifier / scanning)
├── transfer/   transfer-01.png, ...                 (depositing the box into server)
├── celebrate/  celebrate-01.png, ...                (arms up / trophy)
└── manifest.json
```

Optional extra: `wave/` (greeting).

## Per-state frame counts (suggested)
- walk: 6–8   ·  carry: 6–8   ·  idle: 2–4
- scan: 3–5   ·  transfer: 3–5   ·  celebrate: 3–4

## Hard requirement #2: consistent anchor (this is what makes it fluid)
Within a state, EVERY frame must share:
- the **same canvas size** (recommend **768×768**, robot filling most of it),
- the **same baseline** — feet at the same Y, body horizontally centered.
If the robot shifts frame-to-frame the walk jitters. Consistent anchor = smooth.

## Style
- Same character, same relative scale across all states.
- `walk` and `carry` must be a **side profile facing right** (mascot travels left→right).

## manifest.json
Fill in the real frame count per state once files are added. Example:

```json
{
  "displayHeight": 320,
  "states": {
    "idle":      { "frames": 4, "fps": 6 },
    "walk":      { "frames": 8, "fps": 12 },
    "carry":     { "frames": 8, "fps": 12 },
    "scan":      { "frames": 4, "fps": 6 },
    "transfer":  { "frames": 5, "fps": 8 },
    "celebrate": { "frames": 4, "fps": 8 }
  }
}
```

When this file + the frames exist, the `FloatingMascot` island activates.
Until then it renders nothing (the landing stays clean).
