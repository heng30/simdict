use anyhow::Result;
use log::{debug, info};
use std::{env::args, thread};

slint::include_modules!();

fn main() -> Result<()> {
    env_logger::init();

    let argv: Vec<String> = args().collect();
    let initial_search = if argv.len() > 1 {
        argv[1].clone().trim().to_string()
    } else {
        String::new()
    };

    let ui = MainWindow::new()?;

    let ui_weak = ui.as_weak();
    ui.on_search(move |word| {
        if word.trim().is_empty() {
            return;
        }

        info!("Search requested for: {}", word);

        let result = match fetch_translation(&word) {
            Ok(translation) => translation,
            Err(e) => format!("Error: {}", e),
        };

        if let Some(ui) = ui_weak.upgrade() {
            ui.set_translation(result.into());
        }
    });

    ui.on_quit(move || {
        _ = slint::quit_event_loop();
    });

    if !initial_search.is_empty() {
        ui.set_input_text(initial_search.clone().into());

        let ui_weak = ui.as_weak();
        thread::spawn(move || match fetch_translation(&initial_search) {
            Ok(result) => ui_weak.upgrade_in_event_loop(move |ui| {
                ui.set_translation(result.into());
            }),
            Err(e) => ui_weak.upgrade_in_event_loop(move |ui| {
                ui.set_translation(format!("Error: {e}").into());
            }),
        });
    }

    ui.run()?;

    Ok(())
}

fn fetch_translation(word: &str) -> Result<String> {
    debug!("Fetching translation for: {}", word);

    let url = format!(
        "https://cn.bing.com/dict/search?q={}",
        urlencoding::encode(word)
    );

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.212 Safari/537.36 Edg/90.0.818.62")
        .build()?;

    let response = client.get(&url).send()?;
    let html = response.text()?;

    let result = parse_bing_dict_html(&html);
    info!("Successfully fetched translation for: {}", word);

    Ok(result)
}
fn parse_bing_dict_html(html: &str) -> String {
    let start_pattern = r#"<meta name="description" content=""#;
    let end_pattern = r#"" />"#;

    if let Some(start_pos) = html.find(start_pattern) {
        let after_start = &html[start_pos + start_pattern.len()..];

        if let Some(end_pos) = after_start.find(end_pattern) {
            return after_start[..end_pos].to_string();
        }
    }

    "No Data!".to_string()
}
