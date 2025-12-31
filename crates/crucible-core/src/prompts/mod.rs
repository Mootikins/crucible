//! System prompt templates and model size detection

mod size;
mod templates;

pub use size::ModelSize;
pub use templates::{
    base_prompt_for_size, LARGE_MODEL_PROMPT, MEDIUM_MODEL_PROMPT, SMALL_MODEL_PROMPT,
};
