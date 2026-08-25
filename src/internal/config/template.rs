use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::RwLock;

use crate::internal::git::ParsedRepoUrl;
use serde::Deserialize;
use serde::Serialize;
use tera::Kwargs;
use tera::Tera;
use tera::Value;

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

pub fn make_partial_resolve_fn(
    arc_context: Arc<RwLock<tera::Context>>,
) -> impl tera::Function<tera::TeraResult<Value>> + 'static {
    Box::new(
        move |args: Kwargs, _state: &tera::State| -> tera::TeraResult<Value> {
            let handle = args
                .must_get::<String>("handle")
                .map_err(|_| tera::Error::message("partial_resolve: could not parse handle"))?;

            // Get the context from the arc pointer
            let context = arc_context.read().unwrap();

            let repo_object = match context.get("repo") {
                Some(value) => match value.as_map() {
                    Some(value) => value,
                    None => {
                        return Err(tera::Error::message(
                            "partial_resolve: no repo in context",
                        ));
                    }
                },
                None => return Err(tera::Error::message("partial_resolve: no repo in context")),
            };

            let repo_handle = match repo_object
                .iter()
                .find_map(|(key, value)| (key.as_str() == Some("handle")).then_some(value))
            {
                Some(value) => match value.as_str() {
                    Some(value) => value,
                    None => {
                        return Err(tera::Error::message(
                            "partial_resolve: no handle in repo",
                        ));
                    }
                },
                None => return Err(tera::Error::message("partial_resolve: no handle in repo")),
            };

            let repo = match Repo::parse(repo_handle) {
                Ok(repo) => repo,
                Err(_) => {
                    return Err(tera::Error::message(
                        "partial_resolve: could not parse repo_handle",
                    ));
                }
            };

            match repo.partial_resolve(&handle) {
                Ok(value) => Ok(Value::from_serializable(&value.to_string())),
                Err(_) => Ok(Value::none()),
            }
        },
    )
}

pub fn filter_escape_multiline_command(
    value: Value,
    options: Kwargs,
    _state: &tera::State,
) -> tera::TeraResult<Value> {
    let value = match value.as_str() {
        Some(value) => value,
        None if value.is_number() || value.as_bool().is_some() => return Ok(value),
        None => {
            return Err(tera::Error::message(
                "escape_multiline_command: value is not a string",
            ));
        }
    };

    let times = options
        .get::<u64>("times")?
        .unwrap_or(1);

    let mut escaped: String = value.to_string();
    for _ in 0..times {
        escaped = escaped
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('"', "\\\"");
    }
    Ok(Value::from_serializable(&escaped))
}
