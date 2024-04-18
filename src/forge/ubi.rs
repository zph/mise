use std::fmt::Debug;

use crate::cache::CacheManager;
use crate::cli::args::ForgeArg;
use crate::cmd::CmdLineRunner;
use crate::config::{Config, Settings};

use crate::forge::{Forge, ForgeType};
use crate::http::HTTP_FETCH;
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

// Uses ubi for installations https://github.com/houseabsolute/ubi
// it can be installed via mise install cargo:houseabsolute/ubi
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
        let path_with_bin = path_without_quotes + "/bin/";

        // TODO(zph): Extract commonality to apply to each variant
        if self.name().starts_with("http") {
          CmdLineRunner::new("ubi")
              .arg("--url")
              .arg(&format!("{}", self.name()))
              .arg("--in")
              .arg(path_with_bin)
              .with_pr(ctx.pr.as_ref())
              .envs(config.env()?)
              .prepend_path(ctx.ts.list_paths())?
              .execute()?;
        } else {
          CmdLineRunner::new("ubi")
              .arg("--project")
              .arg(&format!("{}", self.name()))
              .arg("--tag").arg(matching_version)
              .arg("--in")
              .arg(path_with_bin)
              .with_pr(ctx.pr.as_ref())
              .envs(config.env()?)
              .prepend_path(ctx.ts.list_paths())?
              .execute()?;
        }

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

fn get_binary_url(n: &str) -> eyre::Result<Url> {
    let n = n.to_lowercase();
    if n.starts_with("https://") || n.starts_with("http://") {
      return Ok(n.parse()?);
    } else {
      let url = format!("https://api.github.com/repos/{n}/releases");
      Ok(url.parse()?)
      }
}
