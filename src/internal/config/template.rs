use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::RwLock;

use git_url_parse::GitUrl;
use serde::Deserialize;
use serde::Serialize;
use tera::Tera;

use crate::internal::cache::PromptsCache;
use crate::internal::git::Repo;
use crate::internal::git_env;
use crate::internal::workdir;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateRepo {
    pub handle: String,
    pub host: String,
    pub org: String,
    pub name: String,
}

impl TemplateRepo {
    pub fn new(url: &GitUrl) -> Self {
        Self {
            handle: url.to_string(),
            host: url.host.clone().unwrap_or_default(),
            org: url.owner.clone().unwrap_or_default(),
            name: url.name.clone(),
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
    template.add_raw_template("askpass", template_str)?;
    template.register_filter("escape_multiline_command", filter_escape_multiline_command);

    if let Some(template_name) = template.templates.keys().next() {
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

    if let Some(template_name) = template.templates.keys().next() {
        let rendered = template.render(template_name, context)?;
        return Ok(rendered);
    }

    Ok("".to_string())
}

pub fn make_partial_resolve_fn(
    arc_context: Arc<RwLock<tera::Context>>,
) -> impl tera::Function + 'static {
    Box::new(
        move |args: &HashMap<String, serde_json::Value>| -> Result<tera::Value, tera::Error> {
            let handle = match args.get("handle") {
                Some(val) => match tera::from_value::<String>(val.clone()) {
                    Ok(v) => v,
                    Err(_) => return Err("partial_resolve: could not parse handle".into()),
                },
                None => return Err("partial_resolve: no handle provided".into()),
            };

            // Get the context from the arc pointer
            let context = arc_context.read().unwrap();

            let repo_object = match context.get("repo") {
                Some(value) => match value.as_object() {
                    Some(value) => value,
                    None => return Err("partial_resolve: no repo in context".into()),
                },
                None => return Err("partial_resolve: no repo in context".into()),
            };

            let repo_handle = match repo_object.get("handle") {
                Some(value) => match value.as_str() {
                    Some(value) => value,
                    None => return Err("partial_resolve: no handle in repo".into()),
                },
                None => return Err("partial_resolve: no handle in repo".into()),
            };

            let repo = match Repo::parse(repo_handle) {
                Ok(repo) => repo,
                Err(_) => return Err("partial_resolve: could not parse repo_handle".into()),
            };

            match repo.partial_resolve(&handle) {
                Ok(value) => Ok(tera::to_value(value.to_string()).unwrap()),
                Err(_) => Ok(tera::Value::Null),
            }
        },
    )
}

pub fn filter_escape_multiline_command(
    value: &tera::Value,
    options: &HashMap<String, tera::Value>,
) -> Result<tera::Value, tera::Error> {
    let value = match value {
        tera::Value::String(value) => value,
        tera::Value::Number(_) | tera::Value::Bool(_) => return Ok(value.clone()),
        _ => return Err("escape_multiline_command: value is not a string".into()),
    };

    let times = match options.get("times") {
        Some(value) => match value {
            tera::Value::Number(value) => value.as_u64().unwrap_or(1),
            _ => return Err("escape_multiline_command: times is not a number".into()),
        },
        None => 1,
    };

    let mut escaped: String = value.to_string();
    for _ in 0..times {
        escaped = escaped
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('"', "\\\"");
    }
    Ok(tera::Value::String(escaped))
}
