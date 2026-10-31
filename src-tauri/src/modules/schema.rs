//! A module's settings form, described as data. The UI renders it generically,
//! so a new module gets a working form without any screen code.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
}

pub const fn choice(value: &'static str, label: &'static str) -> Choice {
    Choice { value, label }
}

/// Show a field only while another field has a given value.
#[derive(Clone, Debug, Serialize)]
pub struct ShowWhen {
    pub key: &'static str,
    pub value: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Field {
    /// One of a few choices, shown as a segmented control.
    #[serde(rename_all = "camelCase")]
    Select {
        key: &'static str,
        label: &'static str,
        options: Vec<Choice>,
        default: &'static str,
        help: Option<&'static str>,
    },
    #[serde(rename_all = "camelCase")]
    Number {
        key: &'static str,
        label: &'static str,
        min: f64,
        max: f64,
        step: f64,
        unit: &'static str,
        default: f64,
        help: Option<&'static str>,
        show_when: Option<ShowWhen>,
        /// Render as a slider rather than a number box.
        slider: bool,
    },
    /// On or off, as a checkbox.
    #[serde(rename_all = "camelCase")]
    Toggle {
        key: &'static str,
        label: &'static str,
        default: bool,
        help: Option<&'static str>,
    },
    /// Multi-line text, sent as one string per non-empty line.
    #[serde(rename_all = "camelCase")]
    Lines {
        key: &'static str,
        label: &'static str,
        placeholder: &'static str,
        help: Option<&'static str>,
    },
    /// The shared encode settings panel (frame rate, quality or bitrate…).
    #[serde(rename_all = "camelCase")]
    Encode { key: &'static str },
    /// The chosen files as an ordered list, each with optional start and end
    /// times. Sent as `[{start, end}]` in the same order as the inputs.
    #[serde(rename_all = "camelCase")]
    TrimList { key: &'static str },
}
