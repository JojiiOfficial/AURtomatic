use std::cmp::Ordering;
use std::env;
use std::path::Path;

use aur_client::aur;

fn main() {
    let path = match env::args().skip(1).next() {
        Some(p) => p,
        None => {
            println!("Provide a path!");
            return;
        }
    };

    let path = Path::new(&path);

    for i in path.read_dir().unwrap() {
        let file_name = i.unwrap().file_name();
        let file_name = file_name.to_str().unwrap();

        if !file_name.ends_with(".zst") && !file_name.ends_with(".xz") {
            continue;
        }

        let info = pkginfo::new(Path::new(path).join(file_name).to_str().unwrap());
        if info.is_err() {
            continue;
        }

        let local_pkg_info = info.unwrap();
        let remote_pkg_results = { aur::info(&[&local_pkg_info.pkg_name]).unwrap().results };
        if remote_pkg_results.len() == 0 {
            println!("Not found: {}", local_pkg_info.pkg_name);
            continue;
        }
        let remote_pkg = remote_pkg_results.get(0).unwrap();

        let local_ver = alpm::Version::new(&local_pkg_info.pkg_ver);
        let rem_ver = alpm::Version::new(&remote_pkg.Version);

        let version_diff = alpm::Version::cmp(&local_ver, &rem_ver);

        if version_diff != Ordering::Less {
            continue;
        }

        println!(
            "{}: local: {} Remote: {} {}",
            local_pkg_info.pkg_name,
            local_pkg_info.pkg_ver,
            remote_pkg.Version,
            {
                match version_diff {
                    Ordering::Less => "(Update available)",
                    Ordering::Greater => "(Local is newer)",
                    Ordering::Equal => "(Up to date)",
                }
            }
        );
    }
}