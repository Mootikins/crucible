use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListJobsParams {}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetJobResultParams {
    pub job_id: String,
}

fn main() {
    let empty_schema = schemars::schema_for!(ListJobsParams);
    let non_empty_schema = schemars::schema_for!(GetJobResultParams);
    
    println!("Empty struct schema:");
    println!("{}", serde_json::to_string_pretty(&empty_schema).unwrap());
    println!("\nNon-empty struct schema:");
    println!("{}", serde_json::to_string_pretty(&non_empty_schema).unwrap());
}
