use std::{collections::{BTreeMap, BTreeSet}, path::{Path, PathBuf}, sync::Arc, time::Duration};

use futures::StreamExt;
use indexmap::IndexMap;
use notify::{event::{MetadataKind, ModifyKind}, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _Watcher};
use parking_lot::RwLock;
use shared::StreamBuf;
use tokio::{fs, sync::mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::{emit, external, files::{Files, FilesOp}};

pub struct Watcher {
	watcher: RecommendedWatcher,
	watched: Arc<RwLock<IndexMap<PathBuf, Option<PathBuf>>>>,
}

impl Watcher {
	pub(super) fn start() -> Self {
		let (tx, rx) = mpsc::unbounded_channel();
		let rx = StreamBuf::new(UnboundedReceiverStream::new(rx), Duration::from_millis(300));

		let watcher = RecommendedWatcher::new(
			{
				let tx = tx.clone();
				move |res: Result<notify::Event, notify::Error>| {
					let Ok(event) = res else {
						return;
					};

					let Some(path) = event.paths.first().cloned() else {
						return;
					};

					let parent = path.parent().unwrap_or(&path).to_path_buf();
					match event.kind {
						EventKind::Create(_) => {
							tx.send(parent).ok();
						}
						EventKind::Modify(kind) => {
							match kind {
								ModifyKind::Data(_) => {}
								ModifyKind::Metadata(kind) => match kind {
									MetadataKind::Permissions => {}
									MetadataKind::Ownership => {}
									MetadataKind::Extended => {}
									_ => return,
								},
								ModifyKind::Name(_) => {}
								_ => return,
							};

							tx.send(path).ok();
							tx.send(parent).ok();
						}
						EventKind::Remove(_) => {
							tx.send(path).ok();
							tx.send(parent).ok();
						}
						_ => (),
					}
				}
			},
			Default::default(),
		);

		let instance = Self { watcher: watcher.unwrap(), watched: Default::default() };
		tokio::spawn(Self::changed(rx, instance.watched.clone()));
		instance
	}

	pub(super) fn watch(&mut self, mut to_watch: BTreeSet<PathBuf>) {
		let keys = self.watched.read().keys().cloned().collect::<BTreeSet<_>>();
		for p in keys.difference(&to_watch) {
			self.watcher.unwatch(p).ok();
		}
		for p in to_watch.clone().difference(&keys) {
			if self.watcher.watch(p, RecursiveMode::NonRecursive).is_err() {
				to_watch.remove(p);
			}
		}

		let mut todo = Vec::new();
		let mut watched = self.watched.write();
		*watched = IndexMap::from_iter(to_watch.into_iter().map(|k| {
			if let Some(v) = watched.remove(&k) {
				(k, v)
			} else {
				todo.push(k.clone());
				(k, None)
			}
		}));
		watched.sort_unstable_by(|_, a, _, b| b.cmp(a));

		let watched = self.watched.clone();
		tokio::spawn(async move {
			let mut ext = IndexMap::new();
			for k in todo {
				match fs::canonicalize(&k).await {
					Ok(v) if v != k => {
						ext.insert(k, Some(v));
					}
					_ => {}
				}
			}

			let mut watched = watched.write();
			watched.extend(ext);
			watched.sort_unstable_by(|_, a, _, b| b.cmp(a));
		});
	}

	pub(super) fn trigger_dirs(&self, dirs: &[&Path]) {
		let watched = self.watched.clone();
		let dirs = dirs.iter().map(|p| p.to_path_buf()).collect::<Vec<_>>();
		tokio::spawn(async move {
			for dir in dirs {
				Self::dir_changed(&dir, watched.clone()).await;
			}
		});
	}

	async fn changed(
		mut rx: StreamBuf<UnboundedReceiverStream<PathBuf>>,
		watched: Arc<RwLock<IndexMap<PathBuf, Option<PathBuf>>>>,
	) {
		while let Some(paths) = rx.next().await {
			let (mut files, mut dirs): (Vec<_>, Vec<_>) = Default::default();
			for path in paths.into_iter().collect::<BTreeSet<_>>() {
				if fs::symlink_metadata(&path).await.map(|m| !m.is_dir()).unwrap_or(false) {
					files.push(path);
				} else {
					dirs.push(path);
				}
			}

			Self::file_changed(files.iter().map(AsRef::as_ref).collect()).await;
			for file in files {
				emit!(Files(FilesOp::IOErr(file)));
			}

			for dir in dirs {
				Self::dir_changed(&dir, watched.clone()).await;
			}
		}
	}

	async fn file_changed(paths: Vec<&Path>) {
		if let Ok(mimes) = external::file(&paths).await {
			emit!(Mimetype(mimes));
		}
	}

	async fn dir_changed(path: &Path, watched: Arc<RwLock<IndexMap<PathBuf, Option<PathBuf>>>>) {
		let linked = watched
			.read()
			.iter()
			.map_while(|(k, v)| v.as_ref().and_then(|v| path.strip_prefix(v).ok()).map(|v| k.join(v)))
			.collect::<Vec<_>>();

		let result = Files::read_dir(path).await;
		if linked.is_empty() {
			emit!(Files(match result {
				Ok(items) => FilesOp::Read(path.into(), items),
				Err(_) => FilesOp::IOErr(path.into()),
			}));
			return;
		}

		for ori in linked {
			emit!(Files(match &result {
				Ok(items) => {
					let files = BTreeMap::from_iter(items.iter().map(|(p, f)| {
						let p = ori.join(p.strip_prefix(path).unwrap());
						let f = f.clone().set_path(&p);
						(p, f)
					}));
					FilesOp::Read(ori, files)
				}
				Err(_) => FilesOp::IOErr(ori),
			}));
		}
	}
}
