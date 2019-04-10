use crate::error::MyError;
use app_dirs::{AppDataType, AppInfo};
use std::collections::HashMap;
use std::fs;
use std::fs::DirEntry;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug)]
pub struct SaveTemplate {
    files: HashMap<PathBuf, String>,
    name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Template {
    files: HashMap<PathBuf, String>,
    name: String,
    location: PathBuf,
}

fn visit_dirs(dir: &Path) -> Result<HashMap<PathBuf, String>, MyError> {
    let mut files = HashMap::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let entries = visit_dirs(&path)?;
                files.extend(entries);
            } else {
                let mut f = fs::File::open(&path)?;
                let mut content = String::new();
                f.read_to_string(&mut content)?;
                files.insert(path, content);
            }
        }
    }
    Ok(files)
}

impl Template {
    fn load_from_disk(name: String, location: PathBuf) -> Result<Template, MyError> {
        let mut files = visit_dirs(&location.join(&name))?;
        Ok(Template {
            name,
            files,
            location,
        })
    }

    fn save_template(template: Template) -> Result<(), MyError> {
        let path = template.location.join(template.name);
        for (location, content) in template.files {
            println!("path inside function {:?}", path.join(&location));
            std::fs::create_dir_all(
                path.join(&location)
                    .parent()
                    .ok_or("Could not detect parent directory")?,
            )?;
            let mut f = std::fs::File::create(path.join(&location))?;
            f.write_all(content.as_bytes())?;
        }
        Ok(())
    }
}

//const APP_INFO: AppInfo = AppInfo {
//    name: "Collabook",
//    author: "Akhil",
//};

fn get_variable(name: String) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempdir::TempDir;

    fn setup_template(name: &str) -> TempDir {
        let temp_dir = TempDir::new("test_dir").unwrap();
        let mut template = HashMap::new();
        template.insert(
            PathBuf::from("Book/Chap1/Sec1"),
            "Content {{var_1}}".to_owned(),
        );
        template.insert(
            PathBuf::from("Book/Chap2/Sec1"),
            "Content {{var_2}}".to_owned(),
        );
        let save_template_request = Template {
            files: template,
            name: name.to_owned(),
            location: temp_dir.path().to_owned(),
        };
        Template::save_template(save_template_request).unwrap();
        temp_dir
    }

    #[test]
    fn load_template_works() {
        let td = setup_template("test_load");
        let template =
            Template::load_from_disk("test_load".to_owned(), td.path().to_owned()).unwrap();
        println!("{:?}", template.files);
        assert_eq!(
            template
                .files
                .get(&td.path().join("test_load/Book/Chap1/Sec1")),
            Some(&"Content {{var_1}}".to_owned())
        );
    }

    #[test]
    fn save_template_saves_files() {
        let td = setup_template("test1");
        let path = td.path();

        println!("{:?}", path);
        assert_eq!(path.join("test1/Book/Chap1/Sec1").exists(), true);
        assert_eq!(path.join("test1/Book/Chap2/Sec1").exists(), true);
        let mut f = std::fs::File::open(path.join("test1/Book/Chap1/Sec1")).unwrap();
        let mut f2 = std::fs::File::open(path.join("test1/Book/Chap2/Sec1")).unwrap();
        let mut content = String::new();
        f.read_to_string(&mut content).unwrap();
        assert_eq!("Content {{var_1}}", content);
        let mut content2 = String::new();
        f2.read_to_string(&mut content2).unwrap();
        assert_eq!("Content {{var_2}}", content2);
    }

    #[test]
    fn get_variables_from_template_works() {
        let td = setup_template("test2");
        let variables = get_variable("test2".to_owned());
    }
}
