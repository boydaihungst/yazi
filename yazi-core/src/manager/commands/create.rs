use std::path::{PathBuf, MAIN_SEPARATOR};

use tokio::fs;
use yazi_config::popup::InputCfg;
use yazi_shared::{event::Exec, fs::{File, FilesOp, Url}};

use crate::{input::Input, manager::Manager};

pub struct Opt {
	force: bool,
}

impl From<Exec> for Opt {
	fn from(e: Exec) -> Self { Self { force: e.named.contains_key("force") } }
}

impl Manager {
	pub fn create(&self, opt: impl Into<Opt>) {
		let opt = opt.into() as Opt;
		let cwd = self.cwd().to_owned();
		tokio::spawn(async move {
			let mut result = Input::_show(InputCfg::create());
			let Some(Ok(name)) = result.recv().await else {
				return Ok(());
			};

			let path = cwd.join(&name);
			if !opt.force && fs::symlink_metadata(&path).await.is_ok() {
				match Input::_show(InputCfg::overwrite()).recv().await {
					Some(Ok(c)) if c == "y" || c == "Y" => (),
					_ => return Ok(()),
				}
			}

			if name.ends_with(MAIN_SEPARATOR) {
				fs::create_dir_all(&path).await?;
			} else {
				fs::create_dir_all(&path.parent().unwrap()).await.ok();
				fs::File::create(&path).await?;
			}

			let child =
				Url::from(path.components().take(cwd.components().count() + 1).collect::<PathBuf>());
			if let Ok(f) = File::from(child.clone()).await {
				FilesOp::Creating(cwd, vec![f]).emit();
				Manager::_hover(Some(child));
			}
			Ok::<(), anyhow::Error>(())
		});
	}
}
