use serde::{Serialize, Serializer};

const ERROR_SOURCE_DEPTH_MAX: u32 = 8;

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error("api error: {status} {message}")]
    API { status: u16, message: String },

    #[error("audio error: {0}")]
    Audio(String),

    #[error("clipboard error: {0}")]
    Clipboard(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    IO(#[from] std::io::Error),

    #[error("keystroke error: {0}")]
    Keystroke(String),

    #[error("network error: {}", error_chain_describe(.0))]
    Network(#[from] reqwest::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("shortcut error: {0}")]
    Shortcut(String),

    #[error("task error: {0}")]
    Task(String),
}

impl AppError {
    pub(crate) fn detail(&self) -> String {
        let detail = match self {
            Self::API { status, message } => format!("{message} ({status})"),
            Self::Audio(message)
            | Self::Clipboard(message)
            | Self::Config(message)
            | Self::Keystroke(message)
            | Self::Shortcut(message)
            | Self::Task(message) => message.clone(),
            Self::IO(error) => error.to_string(),
            Self::Network(error) => error_chain_describe(error),
            Self::Serde(error) => error.to_string(),
        };

        debug_assert_ne!(detail, "");

        sentence_case(&detail)
    }

    pub(crate) const fn title(&self) -> &'static str {
        match self {
            Self::API { .. } => "The speech service rejected the request",
            Self::Audio(_) => "The microphone could not be used",
            Self::Clipboard(_) | Self::Keystroke(_) => "ora could not paste the text",
            Self::Config(_) => "ora is not configured",
            Self::IO(_) | Self::Serde(_) | Self::Task(_) => "An internal error occurred",
            Self::Network(_) => "The speech service could not be reached",
            Self::Shortcut(_) => "The dictation shortcut failed",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub(crate) type AppResult<T> = Result<T, AppError>;

fn sentence_case(text: &str) -> String {
    let mut characters = text.chars();

    let Some(first) = characters.next() else {
        return String::new();
    };

    let rest = characters.as_str();
    let sentence: String = first.to_uppercase().chain(rest.chars()).collect();

    debug_assert!(sentence.ends_with(rest));
    debug_assert!(sentence.len() > rest.len());

    sentence
}

pub(crate) fn error_chain_describe(error: &dyn std::error::Error) -> String {
    let mut described = error.to_string();
    let mut source = error.source();
    let mut depth: u32 = 0;

    while let Some(cause) = source {
        if depth >= ERROR_SOURCE_DEPTH_MAX {
            break;
        }

        let text = cause.to_string();

        if !described.contains(&text) {
            described.push_str(": ");
            described.push_str(&text);
        }

        source = cause.source();
        depth += 1;
    }

    debug_assert!(depth <= ERROR_SOURCE_DEPTH_MAX);
    debug_assert_ne!(described, "");

    described
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Layered {
        source: Option<Box<Self>>,
        text: &'static str,
    }

    impl std::fmt::Display for Layered {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.text)
        }
    }

    impl std::error::Error for Layered {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.source.as_deref().map(|inner| -> &(dyn std::error::Error + 'static) { inner })
        }
    }

    fn layered(texts: &[&'static str]) -> Layered {
        let mut error = Layered { source: None, text: texts[texts.len() - 1] };

        for text in texts.iter().rev().skip(1) {
            error = Layered { source: Some(Box::new(error)), text };
        }

        error
    }

    #[test]
    fn a_chain_lists_each_cause_once_and_stops_at_its_depth_limit() {
        let chain = layered(&["sending request", "connection reset", "broken pipe"]);
        let repeated = layered(&["timed out", "timed out"]);

        let deep =
            layered(&["l0", "l1", "l2", "l3", "l4", "l5", "l6", "l7", "l8", "l9", "l10", "l11"]);

        assert_eq!(error_chain_describe(&chain), "sending request: connection reset: broken pipe");
        assert_eq!(error_chain_describe(&repeated), "timed out");
        assert_eq!(error_chain_describe(&deep), "l0: l1: l2: l3: l4: l5: l6: l7: l8");
    }

    #[test]
    fn a_detail_drops_the_category_that_the_display_text_carries() {
        let api = AppError::API { status: 401, message: "the key was rejected".to_owned() };
        let audio = AppError::Audio("no microphone".to_owned());

        assert_eq!(api.to_string(), "api error: 401 the key was rejected");
        assert_eq!(api.detail(), "The key was rejected (401)");
        assert_eq!(audio.detail(), "No microphone");
        assert_eq!(sentence_case("élan"), "Élan");
        assert_eq!(sentence_case("ßtraße"), "SStraße");
        assert_eq!(sentence_case(""), "");
        assert_ne!(api.title(), audio.title());
    }

    #[test]
    fn an_error_crosses_the_ipc_boundary_as_its_display_text() {
        let error = AppError::Config("no key".to_owned());

        assert_eq!(serde_json::to_string(&error).unwrap(), r#""config error: no key""#);
    }
}
