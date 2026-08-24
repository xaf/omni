use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::RwLock;

use crate::internal::git::ParsedRepoUrl;
use serde::Serialize;
use tera::Tera;

use crate::internal::cache::PromptsCache;
use crate::internal::git::Repo;
use crate::internal::git_env;
use crate::internal::workdir;

#[derive(Debug, Serialize, Clone)]
pub struct TemplateRepo {
    pub handle: String,
    pub host: String,
    pub org: String,
    pub name: String,
}

impl TemplateRepo {
    pub fn new(url: &ParsedRepoUrl) -> Self {
        let host = url.host.clone().unwrap_or_default();
        let org = url.owner.clone().unwrap_or_default();
        let name = url.name.clone();
        Self {
            handle: url.raw.clone(),
            host,
            org,
            name,
        }
    }
}

pub fn config_template_context<T: AsRef<str>>(path: T) -> tera::Context {
    let mut context = tera::Context::new();
    let path = path.as_ref();

    // Load context for the work directory
    let wd = workdir(path);
    if let Some(id) = wd.id() {
        context.insert("id", &id);
    }
    if let Some(root) = wd.root() {
        context.insert("root", &root);
    }

    // Load context for the git environment
    let git = git_env(path);
    if let Some(url) = git.url() {
        let repo = TemplateRepo::new(&url);
        context.insert("repo", &repo);
    }

    // Load context for the environment
    let env = std::env::vars().collect::<HashMap<String, String>>();
    context.insert("env", &env);

    // Load context for the user prompts
    let prompts = PromptsCache::get().answers(path);
    context.insert("prompts", &prompts);

    context
}

pub fn tera_render_error_message(err: tera::Error) -> String {
    // Get the deepest source of the error
    let mut source: &dyn Error = &err;
    while let Some(err) = source.source() {
        source = err;
    }
    let errmsg = source.to_string();

    // Make sure the first letter is not a capital
    let errmsg = errmsg
        .chars()
        .next()
        .unwrap()
        .to_lowercase()
        .collect::<String>()
        + &errmsg[1..];

    errmsg
}

pub fn render_askpass_template(context: &tera::Context) -> Result<String, tera::Error> {
    let template_str = include_str!("../../../templates/askpass.sh.tmpl");

    let mut template = Tera::default();
    template.register_filter("escape_multiline_command", filter_escape_multiline_command);
    template.add_raw_template("askpass", template_str)?;

    if let Some(template_name) = template.get_template_names().next() {
        let rendered = template.render(template_name, context)?;
        return Ok(rendered);
    }

    Ok("".to_string())
}

pub fn render_config_template(
    template: &tera::Tera,
    context: &tera::Context,
) -> Result<String, tera::Error> {
    let arc_context = Arc::new(RwLock::new(context.clone()));
    let mut template = template.clone();

    template.register_function(
        "partial_resolve",
        make_partial_resolve_fn(Arc::clone(&arc_context)),
    );

    if let Some(template_name) = template.get_template_names().next() {
        let rendered = template.render(template_name, context)?;
        return Ok(rendered);
    }

    Ok("".to_string())
}

pub fn register_partial_resolve_placeholder(template: &mut Tera) {
    template.register_function(
        "partial_resolve",
        |_args: tera::Kwargs, _state: &tera::State| tera::Value::none(),
    );
}

pub fn make_partial_resolve_fn(
    arc_context: Arc<RwLock<tera::Context>>,
) -> impl tera::Function<Result<tera::Value, tera::Error>> + 'static {
    move |args: tera::Kwargs, _state: &tera::State| -> Result<tera::Value, tera::Error> {
        let handle = args.must_get::<&str>("handle")?;

        // Get the context from the arc pointer
        let context = arc_context.read().unwrap();

        let repo_object = match context.get("repo") {
            Some(value) => match value.as_map() {
                Some(value) => value,
                None => return Err(tera::Error::message("partial_resolve: no repo in context")),
            },
            None => return Err(tera::Error::message("partial_resolve: no repo in context")),
        };

        let repo_handle = repo_object
            .iter()
            .find(|(key, _)| key.as_str() == Some("handle"))
            .and_then(|(_, value)| value.as_str())
            .ok_or_else(|| tera::Error::message("partial_resolve: no handle in repo"))?;

        let repo = match Repo::parse(repo_handle) {
            Ok(repo) => repo,
            Err(_) => return Err(tera::Error::message("partial_resolve: could not parse repo_handle")),
        };

        match repo.partial_resolve(&handle) {
            Ok(value) => Ok(tera::Value::normal_string(&value.to_string())),
            Err(_) => Ok(tera::Value::none()),
        }
    }
}

pub fn filter_escape_multiline_command(
    value: tera::Value,
    options: tera::Kwargs,
    _state: &tera::State,
) -> Result<tera::Value, tera::Error> {
    let value = match value.kind() {
        tera::value::ValueKind::String => value.as_str().unwrap(),
        tera::value::ValueKind::U64
        | tera::value::ValueKind::I64
        | tera::value::ValueKind::U128
        | tera::value::ValueKind::I128
        | tera::value::ValueKind::F64
        | tera::value::ValueKind::Bool => return Ok(value),
        _ => return Err(tera::Error::message("escape_multiline_command: value is not a string")),
    };

    let times = options.get::<u64>("times")?.unwrap_or(1);

    let mut escaped: String = value.to_string();
    for _ in 0..times {
        escaped = escaped
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('"', "\\\"");
    }
    Ok(tera::Value::normal_string(&escaped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_documented_variables_and_conditionals() {
        let mut template = Tera::default();
        template
            .add_raw_template(
                "documented",
                r#"{% if prompts.team == "team1" or prompts.team == "team2" %}{{ id }}|{{ root }}|{{ repo.host }}/{{ repo.org }}/{{ repo.name }}|{{ env.HOME }}{% endif %}"#,
            )
            .unwrap();

        let mut context = tera::Context::new();
        context.insert("id", "omni");
        context.insert("root", "/work/omni");
        context.insert(
            "repo",
            &json!({
                "handle": "https://github.com/omnicli/omni.git",
                "host": "github.com",
                "org": "omnicli",
                "name": "omni",
            }),
        );
        context.insert("env", &json!({"HOME": "/home/test"}));
        context.insert("prompts", &json!({"team": "team1"}));

        assert_eq!(
            render_config_template(&template, &context).unwrap(),
            "omni|/work/omni|github.com/omnicli/omni|/home/test"
        );
    }

    #[test]
    fn partial_resolve_can_be_registered_before_template_parsing() {
        let mut template = Tera::default();
        register_partial_resolve_placeholder(&mut template);
        template
            .add_raw_template(
                "partial_resolve",
                r#"{{ partial_resolve(handle="other-repo") }}"#,
            )
            .unwrap();

        let mut context = tera::Context::new();
        context.insert(
            "repo",
            &json!({"handle": "https://github.com/omnicli/omni.git"}),
        );

        assert_eq!(
            render_config_template(&template, &context).unwrap(),
            "https://github.com/omnicli/other-repo"
        );
    }
}
