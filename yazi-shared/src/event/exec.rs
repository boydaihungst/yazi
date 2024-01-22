use std::{any::Any, collections::BTreeMap, fmt::{self, Display}, mem};

#[derive(Debug, Default)]
pub struct Exec {
	pub cmd:   String,
	pub args:  Vec<String>,
	pub named: BTreeMap<String, String>,
	pub data:  Option<Box<dyn Any + Send>>,
}

impl Exec {
	#[inline]
	pub fn call(cwd: &str, args: Vec<String>) -> Self {
		Exec { cmd: cwd.to_owned(), args, ..Default::default() }
	}

	#[inline]
	pub fn call_named(cwd: &str, named: BTreeMap<String, String>) -> Self {
		Exec { cmd: cwd.to_owned(), named, ..Default::default() }
	}

	#[inline]
	pub fn with(mut self, name: impl ToString, value: impl ToString) -> Self {
		self.named.insert(name.to_string(), value.to_string());
		self
	}

	#[inline]
	pub fn with_bool(mut self, name: impl ToString, state: bool) -> Self {
		if state {
			self.named.insert(name.to_string(), Default::default());
		}
		self
	}

	#[inline]
	pub fn with_data(mut self, data: impl Any + Send) -> Self {
		self.data = Some(Box::new(data));
		self
	}

	#[inline]
	pub fn take_data<T: 'static>(&mut self) -> Option<T> {
		self.data.take().and_then(|d| d.downcast::<T>().ok()).map(|d| *d)
	}

	#[inline]
	pub fn take_first(&mut self) -> Option<String> {
		if self.args.is_empty() { None } else { Some(mem::take(&mut self.args[0])) }
	}

	#[inline]
	pub fn take_name(&mut self, name: &str) -> Option<String> { self.named.remove(name) }

	#[inline]
	pub fn clone_without_data(&self) -> Self {
		Self {
			cmd: self.cmd.clone(),
			args: self.args.clone(),
			named: self.named.clone(),
			..Default::default()
		}
	}
}

impl Display for Exec {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.cmd)?;
		if !self.args.is_empty() {
			write!(f, " {}", self.args.join(" "))?;
		}
		for (k, v) in &self.named {
			write!(f, " --{k}")?;
			if !v.is_empty() {
				write!(f, "={v}")?;
			}
		}
		Ok(())
	}
}
