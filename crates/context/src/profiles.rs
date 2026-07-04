use crate::AppInfo;

/// What kind of app the user is dictating into; drives tone and, later,
/// injection strategy (terminals get keystrokes, never paste-with-enter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppProfile {
    Chat,
    Email,
    Coding,
    Terminal,
    Document,
    Generic,
}

const CHAT_APPS: &[&str] = &[
    "slack", "discord", "teams", "telegram", "whatsapp", "signal",
];
const EMAIL_APPS: &[&str] = &["outlook", "olk", "thunderbird", "mailspring", "em client"];
const CODING_APPS: &[&str] = &[
    "code",
    "cursor",
    "windsurf",
    "devenv",
    "idea64",
    "pycharm64",
    "rider64",
    "clion64",
    "goland64",
    "webstorm64",
    "zed",
    "sublime_text",
    "neovide",
];
const TERMINAL_APPS: &[&str] = &[
    "windowsterminal",
    "wt",
    "cmd",
    "powershell",
    "pwsh",
    "conhost",
    "alacritty",
    "wezterm-gui",
    "hyper",
    "tabby",
];
const DOCUMENT_APPS: &[&str] = &["winword", "notion", "obsidian", "typora", "onenote"];
const BROWSERS: &[&str] = &[
    "chrome", "msedge", "firefox", "brave", "opera", "arc", "vivaldi",
];

/// Classify the focused app. Process name first; for browsers the window
/// title (which carries the page title) decides.
pub fn classify(info: &AppInfo) -> AppProfile {
    let stem = info.process.to_ascii_lowercase();
    let stem = stem.trim_end_matches(".exe");
    let title = info.title.to_ascii_lowercase();

    let matches = |names: &[&str]| names.iter().any(|n| stem == *n);

    if matches(CHAT_APPS) {
        return AppProfile::Chat;
    }
    if matches(EMAIL_APPS) {
        return AppProfile::Email;
    }
    if matches(CODING_APPS) {
        return AppProfile::Coding;
    }
    if matches(TERMINAL_APPS) {
        return AppProfile::Terminal;
    }
    if matches(DOCUMENT_APPS) {
        return AppProfile::Document;
    }
    if matches(BROWSERS) {
        if title.contains("gmail") || title.contains("outlook") || title.contains("compose") {
            return AppProfile::Email;
        }
        if title.contains("google docs") || title.contains("- docs") || title.contains("notion") {
            return AppProfile::Document;
        }
        if title.contains("slack") || title.contains("discord") || title.contains("web.whatsapp") {
            return AppProfile::Chat;
        }
        if title.contains("github") || title.contains("colab") || title.contains("codepen") {
            return AppProfile::Coding;
        }
    }
    AppProfile::Generic
}

/// The tone block injected into the cleanup prompt (design doc §7 layer 2).
pub fn tone_for(profile: AppProfile) -> Option<&'static str> {
    match profile {
        AppProfile::Chat => Some(
            "You are formatting a chat message (Slack/Discord style). Keep it casual and \
             concise. No greetings, no sign-offs. Short sentences are fine.",
        ),
        AppProfile::Email => Some(
            "You are formatting email text. Use complete sentences and a professional but \
             warm tone. Preserve any greeting or sign-off the speaker dictated.",
        ),
        AppProfile::Coding | AppProfile::Terminal => Some(
            "You are formatting text inside a code editor or terminal. Be literal: preserve \
             identifiers, filenames, symbols, and spacing exactly. Never use smart quotes or \
             add punctuation to code-like content.",
        ),
        AppProfile::Document => Some(
            "You are formatting document prose. Well-structured sentences and paragraphs; \
             use headings or lists only when the speaker clearly dictated them.",
        ),
        AppProfile::Generic => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(process: &str, title: &str) -> AppInfo {
        AppInfo {
            process: process.into(),
            title: title.into(),
        }
    }

    #[test]
    fn classifies_by_process() {
        assert_eq!(
            classify(&info("slack.exe", "general - Slack")),
            AppProfile::Chat
        );
        assert_eq!(
            classify(&info("Cursor.exe", "main.rs — proj")),
            AppProfile::Coding
        );
        assert_eq!(
            classify(&info("WindowsTerminal.exe", "pwsh")),
            AppProfile::Terminal
        );
        assert_eq!(
            classify(&info("WINWORD.EXE", "report.docx")),
            AppProfile::Document
        );
        assert_eq!(classify(&info("olk.exe", "Inbox")), AppProfile::Email);
        assert_eq!(
            classify(&info("someapp.exe", "whatever")),
            AppProfile::Generic
        );
    }

    #[test]
    fn classifies_browser_tabs_by_title() {
        assert_eq!(
            classify(&info(
                "chrome.exe",
                "Inbox (3) - me@x.com - Gmail — Google Chrome"
            )),
            AppProfile::Email
        );
        assert_eq!(
            classify(&info("msedge.exe", "Q3 Plan - Google Docs")),
            AppProfile::Document
        );
        assert_eq!(
            classify(&info("firefox.exe", "#general | Acme Slack")),
            AppProfile::Chat
        );
        assert_eq!(
            classify(&info("chrome.exe", "Wikipedia — the free encyclopedia")),
            AppProfile::Generic
        );
    }

    #[test]
    fn tone_blocks_exist_where_expected() {
        assert!(tone_for(AppProfile::Chat).unwrap().contains("casual"));
        assert!(tone_for(AppProfile::Coding).unwrap().contains("literal"));
        assert!(tone_for(AppProfile::Generic).is_none());
    }

    #[test]
    fn case_insensitive_process_matching() {
        assert_eq!(classify(&info("Slack.exe", "x")), AppProfile::Chat);
        assert_eq!(classify(&info("POWERSHELL.EXE", "x")), AppProfile::Terminal);
    }
}
