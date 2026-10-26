//! `wokwi.toml`, built with `toml_edit`.
//!
//! The file is only emitted when the `wokwi` option is on. The `[[chip]]`
//! section a Wokwi custom chip needs returns in phase 3 with the bus examples
//! that ship one.

use anyhow::Result;
use espforge_elements::Meta;
use toml_edit::{DocumentMut, Item, Table, value};

use super::chip::Chip;

/// Wokwi's serial monitor part id, used by `diagram.json` connections.
pub const SERIAL_MONITOR: &str = "$serialMonitor";

/// Wokwi board id for a chip (this used to be an if-chain in the template).
pub fn board(chip: &str) -> &'static str {
    match chip {
        "esp32c3" => "board-esp32-c3-devkitm-1",
        "esp32c6" => "board-esp32-c6-devkitc-1",
        "esp32h2" => "board-esp32-h2-devkitm-1",
        "esp32s2" => "board-esp32-s2-devkitm-1",
        "esp32s3" => "board-esp32-s3-devkitc-1",
        // "esp32" and anything Wokwi has no dedicated board for.
        _ => "board-esp32-devkit-c-v4",
    }
}

/// The emitted `wokwi.toml`.
pub fn build(meta: &Meta, chip: &Chip) -> Result<String> {
    let firmware = format!("target/{}/debug/{}", chip.rust_target, meta.crate_name());
    let mut wokwi = Table::new();
    wokwi["version"] = value(1);
    wokwi["gdbServerPort"] = value(3333);
    wokwi["elf"] = value(firmware.as_str());
    wokwi["firmware"] = value(firmware.as_str());
    let mut doc = DocumentMut::new();
    doc["wokwi"] = Item::Table(wokwi);
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::test_support::{case, chip};
    use espforge_elements::Runtime;

    fn build_wokwi(meta: &Meta, chip_name: &str) -> String {
        build(meta, &chip(chip_name)).unwrap()
    }

    #[test]
    fn the_firmware_paths_name_the_target_and_the_crate() {
        let case = case("esp32c3", Runtime::Blocking, true, &[], &[]);
        let doc: DocumentMut = build_wokwi(&case.meta, "esp32c3").parse().unwrap();
        assert_eq!(doc["wokwi"]["version"].as_integer(), Some(1));
        assert_eq!(doc["wokwi"]["gdbServerPort"].as_integer(), Some(3333));
        assert_eq!(
            doc["wokwi"]["elf"].as_str(),
            Some("target/riscv32imc-unknown-none-elf/debug/hello_world")
        );
        assert_eq!(
            doc["wokwi"]["firmware"].as_str(),
            doc["wokwi"]["elf"].as_str()
        );
    }

    #[test]
    fn the_board_table_matches_the_old_diagram_chain() {
        assert_eq!(board("esp32c3"), "board-esp32-c3-devkitm-1");
        assert_eq!(board("esp32"), "board-esp32-devkit-c-v4");
        assert_eq!(board("esp32s3"), "board-esp32-s3-devkitc-1");
        assert_eq!(board("esp32c61"), "board-esp32-devkit-c-v4");
    }

    #[test]
    fn an_xtensa_project_uses_the_xtensa_target() {
        let case = case("esp32s3", Runtime::Blocking, true, &[], &[]);
        assert!(
            build_wokwi(&case.meta, "esp32s3")
                .contains("target/xtensa-esp32s3-none-elf/debug/hello_world")
        );
    }

}
