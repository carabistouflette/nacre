use zed_extension_api::{self as zed, settings::LspSettings};

struct NacreExtension;

impl zed::Extension for NacreExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        if language_server_id.as_ref() != "nacre" {
            return Err(format!(
                "Unknown Nacre language server: {}",
                language_server_id.as_ref()
            ));
        }

        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
        let (configured_path, args, env) = match settings.binary {
            Some(binary) => (
                binary.path,
                binary.arguments.unwrap_or_else(|| vec!["lsp".to_string()]),
                binary.env.unwrap_or_default(),
            ),
            None => (None, vec!["lsp".to_string()], Default::default()),
        };
        let command = configured_path
            .or_else(|| worktree.which("nacre"))
            .ok_or_else(|| {
                "Nacre compiler `nacre` was not found on PATH; install it or configure lsp.nacre.binary.path.".to_string()
            })?;

        Ok(zed::Command::new(command).args(args).envs(env))
    }
}

zed::register_extension!(NacreExtension);
