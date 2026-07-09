// Decode an audio file straight through rockbox-codecs (no player/output) and
// report progress — isolates the codec from the playback engine. Useful for
// checking HE-AAC (SBR) decoding.
//
//   cargo run --release --example hecheck -- /path/to/he-aac.m4a
use rockbox_codecs::Decoder;

fn main() {
    let path = std::env::args().nth(1).expect("usage: hecheck <file>");
    let mut dec = Decoder::open(&path).expect("open");
    println!("codec = {}", dec.metadata().codec);

    let (mut total, mut chunks, mut rate) = (0usize, 0u64, 0u32);
    while let Some(c) = dec.next_chunk() {
        rate = c.sample_rate;
        total += c.pcm.len();
        chunks += 1;
        if chunks % 100 == 0 {
            let ms = if rate > 0 { total / 2 * 1000 / rate as usize } else { 0 };
            println!(
                "chunks={chunks} samples={total} rate={rate} sample_ms={ms} elapsed_ms={}",
                dec.elapsed().as_millis()
            );
        }
        if chunks >= 4000 {
            println!("(stopping at 4000 chunks)");
            break;
        }
    }
    let ms = if rate > 0 { total / 2 * 1000 / rate as usize } else { 0 };
    println!("DONE chunks={chunks} samples={total} rate={rate} ~{ms}ms status={:?}", dec.status());
}
