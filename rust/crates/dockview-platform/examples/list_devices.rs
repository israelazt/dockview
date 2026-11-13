//! Lists video capture devices and every native stream mode they expose,
//! marking which mode each capture profile would select.
//!
//! Run with: `cargo run -p dockview-core --example list_devices`

use dockview_domain::stream::{select_best, CaptureProfile};
use dockview_platform::mf::{enumerate_stream_types, enumerate_video_devices, MfSession};

fn main() -> windows::core::Result<()> {
    let session = MfSession::start()?;
    let devices = enumerate_video_devices(&session)?;

    if devices.is_empty() {
        println!("No video capture devices found.");
        return Ok(());
    }

    for device in &devices {
        println!("\n{}", device.friendly_name);
        println!("  link: {}", device.symbolic_link);

        let types = match enumerate_stream_types(&session, &device.symbolic_link) {
            Ok(types) => types,
            Err(e) => {
                println!("  could not read stream types: {e}");
                continue;
            }
        };

        let picks = [
            (
                "LowLatency",
                select_best(&types, CaptureProfile::LowLatency),
            ),
            ("Balanced", select_best(&types, CaptureProfile::Balanced)),
            ("Quality", select_best(&types, CaptureProfile::Quality)),
        ];

        for t in &types {
            let marks: Vec<&str> = picks
                .iter()
                .filter(|(_, p)| p.is_some_and(|p| p.type_index == t.type_index))
                .map(|(name, _)| *name)
                .collect();
            let compressed = if t.is_compressed { "compressed" } else { "raw" };
            let marks = if marks.is_empty() {
                String::new()
            } else {
                format!("  <= {}", marks.join(", "))
            };
            println!("  [{:>2}] {t}  ({compressed}){marks}", t.type_index);
        }
    }
    Ok(())
}
