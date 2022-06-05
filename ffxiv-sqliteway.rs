use std::{
	collections::hash_map::HashMap,
	fs, io,
	path::{Path, PathBuf},
};

#[derive(PartialEq, Eq, Clone, Debug)]
enum ColumnType {
	Foreign(String),
	Type(String),
	Key,
}

fn to_column_type(s: String) -> ColumnType {
	let first = s.chars().next();

	// what the fuck is it
	if s == "Row" {
		ColumnType::Type(s)
	} else if let Some(c) = first {
		if ('a'..='z').contains(&c) {
			ColumnType::Type(s)
		} else {
			ColumnType::Foreign(s)
		}
	} else {
		ColumnType::Type(s)
	}
}

fn read_column(
	path: impl AsRef<Path>,
) -> csv::Result<Vec<(String, ColumnType)>> {
	let mut reader = csv::ReaderBuilder::new()
		.delimiter(b',')
		.quote(b'"')
		.has_headers(false)
		.from_path(path)?;

	let records = reader.records().take(3).collect::<Result<Vec<_>, _>>()?;

	let columns = if let [indices, names, types] = records.as_slice() {
		indices
			.iter()
			.zip(names.iter())
			.zip(types.iter())
			.map(|((index, name), ty)| {
				if index == "key" {
					return (name.to_string(), ColumnType::Key);
				}

				let ty = to_column_type(ty.to_string());

				let name = if name.is_empty() {
					format!("Unknown{index}")
				} else {
					name.to_string()
				};

				(name, ty)
			})
			.collect()
	} else {
		return Err(io::Error::from(io::ErrorKind::InvalidData).into());
	};

	Ok(columns)
}

fn dir_list(root: impl AsRef<Path>) -> io::Result<HashMap<String, PathBuf>> {
	let mut table_paths = HashMap::default();

	let mut dirs = vec![root.as_ref().to_path_buf()];

	while let Some(dir) = dirs.pop() {
		for entry in fs::read_dir(dir)? {
			let entry = entry?;
			let path = entry.path();
			let ft = entry.file_type()?;

			if ft.is_dir() {
				dirs.push(path);
			} else if ft.is_file() {
				if let Ok(file_name) = entry.file_name().into_string() {
					if let Some(name) = file_name.strip_suffix(".csv") {
						table_paths.insert(name.to_string(), path);
					}
				}
			}
		}
	}

	Ok(table_paths)
}

fn sqlite_quote(s: &str) -> String {
	format!("\"{s}\"")
}

#[derive(clap::Parser, Debug)]
#[clap(version, about)]
struct Args {
	#[clap(long)]
	from: PathBuf,

	#[clap(long)]
	write_sql: Option<PathBuf>,

	#[clap(long)]
	write_command: Option<PathBuf>,
}

fn write_lines(
	to: &mut impl io::Write,
	lines: impl IntoIterator<Item = String>,
) -> io::Result<()> {
	for line in lines.into_iter() {
		writeln!(to, "{line}")?;
	}

	Ok(())
}

fn main() -> anyhow::Result<()> {
	let args: Args = clap::Parser::parse();

	let mut sqls = Vec::new();
	let mut inserts = Vec::new();

	for (table_name, path) in dir_list(args.from)? {
		// How dare you are putting 3000+ columns
		if table_name == "CharaMakeType" {
			continue;
		}

		let mut foreigns = Vec::new();
		let mut cols = Vec::new();

		for (col, ty) in read_column(&path)? {
			let col = sqlite_quote(&col);
			cols.push(match ty {
				ColumnType::Key => "\"#\" TEXT PRIMARY KEY".to_string(),
				ColumnType::Type(_) => format!("{col} TEXT"),
				ColumnType::Foreign(table) => {
					foreigns.push(format!(
						"FOREIGN KEY({col}) REFERENCES {table}(\"#\")"
					));
					format!("{col} TEXT")
				}
			});
		}

		cols.append(&mut foreigns);

		let sql = format!(
			"CREATE TABLE {table_name} ({}) STRICT, WITHOUT ROWID;",
			cols.join(",")
		);
		let insert = format!(
			".import --skip 3 --csv {} {table_name}",
			path.to_string_lossy()
		);

		sqls.push(sql);
		inserts.push(insert);
	}

	if let Some(path) = args.write_sql {
		let mut file = fs::File::create(path)?;
		write_lines(&mut file, sqls)?;
	} else {
		let mut stdout = io::stdout().lock();
		write_lines(&mut stdout, sqls)?;
	}

	if let Some(path) = args.write_command {
		let mut file = fs::File::create(path)?;
		write_lines(&mut file, inserts)?;
	} else {
		let mut stdout = io::stdout().lock();
		write_lines(&mut stdout, inserts)?;
	}

	Ok(())
}
