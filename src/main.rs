use std::{
    borrow::Borrow,
    collections::HashMap,
    fs::{copy, create_dir_all, remove_dir_all, File},
    io::BufReader,
    os::windows::fs::symlink_file,
    path::PathBuf,
};

use chrono::{DateTime, Datelike, Utc};

#[derive(Clone)]
struct Tag {
    pk: i64,
    name: String,
}

#[derive(Clone)]
struct Correspondent {
    pk: i64,
    name: String, // fields[].name
}

struct Document {
    pk: i64,
    file_name: String,                    // __exported_file_name__
    archive_name: String,                 // __exported_archive_name__
    created: DateTime<Utc>,               // fields[].created
    correspondent: Option<Correspondent>, // fields[].correspondent
    tags: Vec<Tag>,                       // fields[].tags[]
}

fn main() -> anyhow::Result<()> {
    let root_dir = r"C:\repos\paperless-ngx\docker\compose\export\";

    for kind in ["files", "by_tag", "by_year", "by_correspondent"] {
        let _ = remove_dir_all(format!(r"{root_dir}\{kind}"));
    }

    let mut tags = HashMap::new();
    let mut correspondents = HashMap::new();
    let mut documents = HashMap::new();

    let manifest_path: PathBuf = PathBuf::from_iter(&[root_dir, "manifest.json"])
        .iter()
        .collect();

    if let Ok(manifest_file) = File::open(manifest_path) {
        let objects: serde_json::Value = serde_json::from_reader(BufReader::new(manifest_file))?;
        for object in objects.as_array().unwrap() {
            let pk = object["pk"].as_i64().unwrap();
            let fields = object["fields"].as_object().unwrap();
            match object["model"].as_str().unwrap() {
                "documents.tag" => {
                    let name = fields
                        .iter()
                        .find(|&(k, _)| k == "name")
                        .expect("tag has name");
                    tags.insert(
                        pk,
                        Tag {
                            pk,
                            name: name.1.as_str().unwrap().into(),
                        },
                    );
                }
                "documents.correspondent" => {
                    let name = fields
                        .iter()
                        .find(|&(k, _)| k == "name")
                        .expect("correspondent has name");
                    correspondents.insert(
                        pk,
                        Correspondent {
                            pk,
                            name: name.1.as_str().unwrap().into(),
                        },
                    );
                }
                "documents.document" => {
                    let created = DateTime::parse_from_rfc3339(
                        fields
                            .iter()
                            .find(|&(k, _)| k == "created")
                            .expect("doc has created")
                            .1
                            .as_str()
                            .expect("created has str value"),
                    )
                    .expect("has rfc3339 date");

                    let correspondent = fields
                        .iter()
                        .find(|&(k, _)| k == "correspondent")
                        .expect("doc has correspondent")
                        .1
                        .as_i64()
                        .expect("created has str value");

                    let tags_obj = fields
                        .iter()
                        .find(|&(k, _)| k == "tags")
                        .expect("doc has tags")
                        .1
                        .as_array()
                        .expect("tags has array value");

                    documents.insert(
                        pk,
                        Document {
                            pk,
                            file_name: object["__exported_file_name__"].as_str().unwrap().into(), // __exported_file_name__
                            archive_name: object["__exported_archive_name__"]
                                .as_str()
                                .unwrap_or(object["__exported_file_name__"].as_str().unwrap())
                                .into(), // __exported_archive_name__
                            created: created.into(), // fields[].created
                            correspondent: correspondents.get(&correspondent).cloned(), // fields[].correspondent
                            tags: tags_obj
                                .iter()
                                .map(|t| tags.get(&t.as_i64().unwrap()).unwrap())
                                .cloned()
                                .collect(), // fields[].tags[]
                        },
                    );
                }
                _ => {}
            }
        }
    }

    let mut num_skipped = 0u64;
    let mut num_copied = 0u64;

    for (_, doc) in documents {
        let tags_str: Vec<_> = doc.tags.iter().map(|t| t.name.as_str()).collect();
        if tags_str
            .iter()
            .any(|t| ["fine", "legal", "private"].contains(t) || t.ends_with("2"))
        {
            num_skipped += 1;
            println!(
                "skipping {} ({})",
                doc.archive_name,
                doc.tags
                    .iter()
                    .map(|t| t.clone().name)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        } else {
            macro_rules! path_from_root {
                ($($xprs:expr),*) => {
                    PathBuf::from_iter(&[root_dir, $($xprs),*])
                        .iter()
                        .collect::<PathBuf>()
                }
            }

            let real_path = path_from_root!(&doc.archive_name);
            let copy_path = path_from_root!("files", &doc.archive_name);
            let by_year = path_from_root!(
                "by_year",
                &doc.created.year().to_string(),
                &doc.archive_name
            );
            let by_correspondent = path_from_root!(
                "by_correspondent",
                &doc.correspondent
                    .map(|c| c.name)
                    .unwrap_or("dummy".to_owned()),
                &doc.archive_name
            );

            let _ = create_dir_all(copy_path.parent().unwrap());
            let _ = create_dir_all(by_year.parent().unwrap());
            let _ = create_dir_all(by_correspondent.parent().unwrap());

            copy(&real_path, &copy_path).expect("create copy of archive pdf");
            symlink_file(&copy_path, &by_year).expect("create symlink (by year)");
            symlink_file(&copy_path, &by_correspondent).expect("create symlink (by correspondent)");

            for tag in &doc.tags {
                let by_tag = path_from_root!("by_tag", &tag.name, &doc.archive_name);
                let _ = create_dir_all(by_tag.parent().unwrap());
                symlink_file(&copy_path, &by_tag).expect("create symlink (by tag)");
            }

            num_copied += 1;
        }
    }

    println!("copied {} files, {} were skipped.", num_copied, num_skipped);
    Ok(())
}
