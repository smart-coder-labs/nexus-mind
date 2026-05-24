/// Backfill embeddings for all memories that don't have one yet.
///
/// Usage:
///   cargo run --bin backfill_embeddings -- --db-path ./data/nexusmind.db
///
/// Safe to re-run: skips memories that already have an embedding.

use clap::Parser;
use nexusmind::{
    db::{connection::connect, migrations},
    embed::{self, EmbedService},
};

#[derive(Parser)]
#[command(about = "Backfill embeddings for memories that don't have one")]
struct Args {
    #[arg(long, env = "DB_PATH", default_value = "./data/nexusmind.db")]
    db_path: String,

    #[arg(long, default_value = "32", help = "Batch size for embedding")]
    batch_size: usize,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    eprintln!("→ Opening DB at {}", args.db_path);
    let conn = connect(&args.db_path)?;
    migrations::run_all(&conn)?;

    // Find memories without an embedding
    let mut stmt = conn.prepare(
        "SELECT id, content FROM memories
         WHERE id NOT IN (SELECT memory_id FROM memory_embeddings)
         ORDER BY created_at ASC",
    )?;

    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;

    let total = rows.len();
    if total == 0 {
        eprintln!("✓ All memories already have embeddings.");
        return Ok(());
    }

    eprintln!("→ Loading embedding model (nomic-embed-text-v1.5)…");
    let svc = EmbedService::init()?;
    eprintln!("✓ Model loaded.");
    eprintln!("→ Backfilling {total} memories in batches of {}…", args.batch_size);

    let mut done = 0;
    for chunk in rows.chunks(args.batch_size) {
        let texts: Vec<&str> = chunk.iter().map(|(_, c)| c.as_str()).collect();
        let vecs = svc.embed_batch(&texts)?;

        for ((id, _), vec) in chunk.iter().zip(vecs.iter()) {
            let blob = embed::serialize(vec);
            conn.execute(
                "INSERT OR REPLACE INTO memory_embeddings (memory_id, embedding) VALUES (?1, ?2)",
                rusqlite::params![id, blob],
            )?;
            done += 1;
        }

        eprintln!("  [{done}/{total}]");
    }

    eprintln!("✓ Done. {done} embeddings written.");
    Ok(())
}
