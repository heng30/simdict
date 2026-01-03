use anyhow::Result;
use log::{debug, info, warn};
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

    match fetch_from_bing(word) {
        Ok(result) => {
            info!("Successfully fetched translation from API for: {}", word);
            return Ok(result);
        }
        Err(e) => {
            warn!("API request failed: {}, falling back to web scraping", e);
        }
    }

    match fetch_from_bing_fallback(word) {
        Ok(result) => {
            info!("Successfully fetched translation from API for: {}", word);
            return Ok(result);
        }
        Err(e) => {
            warn!("API request failed: {}", e);
        }
    }

    Ok("No Data".to_string())
}

fn fetch_from_bing(word: &str) -> Result<String> {
    debug!("Trying Bing API for: {}", word);

    let url = format!(
        "https://cn.bing.com/dict/SerpHoverTrans?q={}",
        urlencoding::encode(word)
    );

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/75.0.3770.100 Safari/537.36")
        .build()?;

    let response = client.get(&url).send()?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "API returned status: {}",
            response.status()
        ));
    }

    let html = response.text()?;
    let result = parse_bing_response(&html).ok_or(anyhow::anyhow!("parse_bing_response failed"))?;

    if result.is_empty() {
        return Err(anyhow::anyhow!("No valid data from API"));
    }

    Ok(result)
}

fn fetch_from_bing_fallback(word: &str) -> Result<String> {
    let url = format!(
        "https://cn.bing.com/dict/search?q={}",
        urlencoding::encode(word)
    );

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.212 Safari/537.36 Edg/90.0.818.62")
        .build()?;

    let response = client.get(&url).send()?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "API returned status: {}",
            response.status()
        ));
    }

    let html = response.text()?;
    let result = parse_bing_response_fallback(&html)
        .ok_or(anyhow::anyhow!("parse_bing_response_fallback failed"))?;

    if result.is_empty() {
        return Err(anyhow::anyhow!("No valid data from API"));
    }

    Ok(result)
}

fn parse_bing_response(html: &str) -> Option<String> {
    let mut result = String::new();

    // 提取音标
    let phonetic_pattern = r#"<span class="ht_attr" lang=".*?">\[(.*?)\] </span>"#;
    if let Some(caps) = regex_search(phonetic_pattern, html) {
        if !caps.is_empty() {
            result.push_str(&format!("· [{}]\n", caps[0].trim()));
        }
    }

    // 提取词性解释
    let explain_pattern = r#"<span class="ht_pos">(.*?)</span><span class="ht_trs">(.*?)</span>"#;
    let mut explains = Vec::new();
    if let Some(matches) = regex_search_all(explain_pattern, html) {
        for caps in matches {
            if caps.len() >= 2 {
                let pos = &caps[0];
                let trs = &caps[1];
                explains.push(format!("· {} {}", pos, trs));
            }
        }
    }

    if explains.is_empty() && result.is_empty() {
        return None;
    }

    for explain in explains {
        result.push_str(&explain);
        result.push_str("\n");
    }

    if result.ends_with('\n') {
        result.pop();
    }

    Some(result)
}

// 简单的正则搜索辅助函数
fn regex_search(pattern: &str, text: &str) -> Option<Vec<String>> {
    use regex::Regex;

    let re = Regex::new(pattern).ok()?;
    re.captures(text).map(|caps| {
        caps.iter()
            .skip(1)
            .filter_map(|m| m.map(|m| m.as_str().to_string()))
            .collect()
    })
}

fn regex_search_all(pattern: &str, text: &str) -> Option<Vec<Vec<String>>> {
    use regex::Regex;

    let re = Regex::new(pattern).ok()?;
    let mut results = Vec::new();

    for caps in re.captures_iter(text) {
        let groups: Vec<String> = caps
            .iter()
            .skip(1)
            .filter_map(|m| m.map(|m| m.as_str().to_string()))
            .collect();

        if !groups.is_empty() {
            results.push(groups);
        }
    }

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}
fn parse_bing_response_fallback(html: &str) -> Option<String> {
    let start_pattern = r#"<meta name="description" content=""#;
    let end_pattern = r#"" />"#;

    if let Some(start_pos) = html.find(start_pattern) {
        let after_start = &html[start_pos + start_pattern.len()..];

        if let Some(end_pos) = after_start.find(end_pattern) {
            return Some(after_start[..end_pos].to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bing_hello() {
        let word = "hello";
        match fetch_from_bing(word) {
            Ok(translation) => {
                println!("Translation:\n{}", translation);
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }

    #[test]
    fn test_bing_fallback_hello() {
        let word = "hello";
        match fetch_from_bing_fallback(word) {
            Ok(translation) => {
                println!("Translation: {}", translation);
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }
}
