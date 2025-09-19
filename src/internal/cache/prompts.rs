use std::collections::HashMap;

use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::git_env;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptsCache {}

impl PromptsCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn add_answer(
        &self,
        prompt_id: &str,
        org: String,
        repo: Option<String>,
        answer: serde_yaml::Value,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/prompts_add_answer.sql"),
            params![prompt_id, org, repo, serde_json::to_string(&answer)?],
        )?;
        Ok(inserted > 0)
    }

    pub fn answers(&self, path: &str) -> HashMap<String, serde_yaml::Value> {
        let git = git_env(path);
        match git.url() {
            Some(url) => match (url.owner.as_deref(), url.name.as_str()) {
                (Some(org), name) if !name.is_empty() => self.get_answers(org, name),
                _ => HashMap::new(),
            },
            None => HashMap::new(),
        }
    }

    pub fn get_answers(&self, org: &str, repo: &str) -> HashMap<String, serde_yaml::Value> {
        // Find all answers matching on the org and for which repo
        // is either matching or none
        let db = CacheManager::get();
        let answers: Vec<(String, String)> = match db.query_as(
            include_str!("database/sql/prompts_get_answers.sql"),
            params![org, repo],
        ) {
            Ok(answers) => answers,
            Err(_) => return HashMap::new(),
        };

        let converted_answers = answers
            .iter()
            .flat_map(|(id, answer)| {
                serde_yaml::from_str::<serde_yaml::Value>(answer)
                    .ok()
                    .map(|answer| (id.clone(), answer))
            })
            .collect::<HashMap<_, _>>();

        let mut answers = HashMap::new();
        for (id, answer) in converted_answers {
            answers.entry(id).or_insert(answer);
        }

        answers
    }
}

#[cfg(test)]
#[path = "prompts_test.rs"]
mod tests;
