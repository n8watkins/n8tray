//! The answer to an action or a pressure notice (spec 5.1 and 5.3).

use serde::{Deserialize, Serialize};

/// An action's answer: an outcome (`200`) or a job (`202`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionResult {
    Outcome {
        outcome: Outcome,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Job { job_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Ok,
    Already,
    Refused,
    Failed,
}

/// A memory pressure level the host pushes (spec 5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PressureLevel {
    Ok,
    Attention,
    Problem,
}

/// The body of `POST /n8/v2/pressure`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pressure {
    pub level: PressureLevel,
}
