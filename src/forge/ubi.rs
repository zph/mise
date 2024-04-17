use std::fmt::Debug;

use crate::cache::CacheManager;
use crate::cli::args::ForgeArg;
use crate::cmd::CmdLineRunner;
use crate::config::{Config, Settings};

use crate::forge::{Forge, ForgeType};
use crate::http::HTTP_FETCH;
use itertools::Itertools;
use url::Url;
use crate::install_context::InstallContext;
use crate::toolset::ToolVersion;
use serde_json::Value;

#[derive(Debug)]
pub struct UBIForge {
    fa: ForgeArg,
    remote_version_cache: CacheManager<Vec<String>>,
    latest_version_cache: CacheManager<Option<String>>,
}

impl Forge for UBIForge {
    fn get_type(&self) -> ForgeType {
        ForgeType::Ubi
    }

    fn fa(&self) -> &ForgeArg {
        &self.fa
    }

    fn get_dependencies(&self, _tv: &ToolVersion) -> eyre::Result<Vec<String>> {
        Ok(vec!["ubi".into()])
    }

    // TODO: v0.0.3 is stripped of 'v' such that it reports incorrectly in tool :-/
    fn list_remote_versions(&self) -> eyre::Result<Vec<String>> {
        self.remote_version_cache
            .get_or_try_init(|| {
                let raw = HTTP_FETCH.get_text(get_binary_url(self.name())?)?;
                let releases: Value = serde_json::from_str(&raw)?;
                let mut versions = vec![];
                for v in releases.as_array().unwrap() {
                    versions.push(v["tag_name"].as_str().unwrap().to_string());
                }
                Ok(versions)
            })
            .cloned()
    }

    fn latest_stable_version(&self) -> eyre::Result<Option<String>> {
        self.latest_version_cache
            .get_or_try_init(|| {
              Ok(Some(self.list_remote_versions()?.last().unwrap().into()))
            })
            .cloned()
    }

    fn install_version_impl(&self, ctx: &InstallContext) -> eyre::Result<()> {
        let config = Config::try_get()?;
        let settings = Settings::get();
        settings.ensure_experimental("ubi backend")?;
        // Workaround because of not knowing how to pull out the value correctly without quoting
        let matching_version = self.list_remote_versions()?.into_iter().find(|v| v.contains(&ctx.tv.version)).unwrap().replace("\"", "");
        let path_without_quotes = ctx.tv.install_path().to_str().unwrap().replace("\"", "");

        CmdLineRunner::new("ubi")
            .arg("--project")
            .arg(&format!("{}", self.name()))
            .arg("--tag")
            .arg(matching_version)
            .arg("--in")
            .arg(path_without_quotes + "/bin")
            .with_pr(ctx.pr.as_ref())
            .envs(config.env()?)
            .prepend_path(ctx.ts.list_paths())?
            .execute()?;

        Ok(())
    }
}

impl UBIForge {
    pub fn new(fa: ForgeArg) -> Self {
        Self {
            remote_version_cache: CacheManager::new(
                fa.cache_path.join("remote_versions.msgpack.z"),
            ),
            latest_version_cache: CacheManager::new(fa.cache_path.join("latest_version.msgpack.z")),
            fa,
        }
    }
}

// TODO: allow to specify a binary based on URL only via UBI flag --url
//   -u, --url <url>            The url of the file to download. This can be provided instead of a
//   project or tag. This will not use the GitHub API, so you will never hit
//   the GitHub API limits. This means you do not need to set a GITHUB_TOKEN
//   env var except for private repos.
fn get_binary_url(n: &str) -> eyre::Result<Url> {
    let n = n.to_lowercase();
    let url = match n.split("/").try_len()? {
        // Support GH shorthand
        2 => format!("https://api.github.com/repos/{n}/releases"),
        // Support GH longhand
        // Support arbitrary URL link
        _ => n,
    };
    Ok(url.parse()?)
}
