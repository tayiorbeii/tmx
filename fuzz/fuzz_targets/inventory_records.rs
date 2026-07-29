#![no_main]

use libfuzzer_sys::fuzz_target;
use tmx::switcher::parser::{parse_clients, parse_panes, parse_sessions, parse_windows};

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = parse_sessions("ep", "gen", text);
        let _ = parse_windows("ep", "gen", text);
        let _ = parse_panes("ep", "gen", text);
        let _ = parse_clients("ep", "gen", "501", text);
    }
});
