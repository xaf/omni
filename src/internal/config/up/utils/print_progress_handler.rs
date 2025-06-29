use std::sync::Arc;
use std::sync::Mutex;

use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::user_interface::StringColor;

#[derive(Debug, Clone)]
pub struct PrintProgressHandler {
    template: String,
    message: Arc<Mutex<String>>,
}

impl PrintProgressHandler {
    pub fn new(desc: String, progress: Option<(usize, usize)>) -> Self {
        let prefix = if let Some((current, total)) = progress {
            let padding = format!("{total}").len();
            format!("[{current:padding$}/{total:padding$}] ")
                .bold()
                .light_black()
        } else {
            "".to_string()
        };

        let template = format!("{prefix}{{}} {desc} {{}}");

        PrintProgressHandler {
            template,
            message: Arc::new(Mutex::new("".to_string())),
        }
    }

    fn set_message(&self, message: impl ToString) {
        let mut lock = self.message.lock().unwrap();
        *lock = message.to_string();
    }

    fn get_message(&self) -> String {
        let lock = self.message.lock().unwrap();
        lock.clone()
    }
}

impl ProgressHandler for PrintProgressHandler {
    fn println(&self, message: String) {
        eprintln!("{message}");
    }

    fn progress(&self, message: String) {
        self.set_message(&message);
        eprintln!(
            "{}",
            self.template
                .replacen("{}", "-".light_black().as_str(), 1)
                .replacen("{}", message.as_str(), 1)
        );
    }

    fn success(&self) {
        self.success_with_message("done".to_string());
    }

    fn success_with_message(&self, message: String) {
        self.set_message(&message);
        eprintln!(
            "{}",
            self.template
                .replacen("{}", "✔".green().as_str(), 1)
                .replacen("{}", message.as_str(), 1)
        );
    }

    fn error(&self) {
        self.error_with_message(self.get_message());
    }

    fn error_with_message(&self, message: String) {
        self.set_message(&message);
        eprintln!(
            "{}",
            self.template
                .replacen("{}", "✖".red().as_str(), 1)
                .replacen("{}", message.red().as_str(), 1)
        );
    }

    fn hide(&self) {
        // do nothing
    }

    fn show(&self) {
        // do nothing
    }
}
