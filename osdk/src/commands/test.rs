// SPDX-License-Identifier: MPL-2.0

use std::fs;

use super::{build::do_build, util::DEFAULT_TARGET_RELPATH};
use crate::{
    base_crate::new_base_crate,
    cli::GdbServerArgs,
    config_manager::{BuildConfig, RunConfig, TestConfig},
    util::{get_cargo_metadata, get_current_crate_info, get_target_directory},
};

pub fn execute_test_command(config: &TestConfig) {
    let crates = get_workspace_default_members();
    for crate_path in crates {
        std::env::set_current_dir(crate_path).unwrap();
        test_current_crate(config);
    }
}

pub fn test_current_crate(config: &TestConfig) {
    let current_crate = get_current_crate_info();
    let ws_target_directory = get_target_directory();
    let osdk_target_directory = ws_target_directory.join(DEFAULT_TARGET_RELPATH);
    let target_crate_dir = osdk_target_directory.join("base");
    new_base_crate(&target_crate_dir, &current_crate.name, &current_crate.path);

    let main_rs_path = target_crate_dir.join("src").join("main.rs");

    let ktest_test_whitelist = match &config.test_name {
        Some(name) => format!(r#"Some(&["{}"])"#, name),
        None => r#"None"#.to_string(),
    };

    let mut ktest_crate_whitelist = vec![current_crate.name];
    if let Some(name) = &config.test_name {
        ktest_crate_whitelist.push(name.clone());
    }

    let ktest_static_var = format!(
        r#"
#[no_mangle]
pub static KTEST_TEST_WHITELIST: Option<&[&str]> = {};
#[no_mangle]
pub static KTEST_CRATE_WHITELIST: Option<&[&str]> = Some(&{:#?});
"#,
        ktest_test_whitelist, ktest_crate_whitelist,
    );

    // Append the ktest static variable to the main.rs file
    let mut main_rs_content = fs::read_to_string(&main_rs_path).unwrap();
    main_rs_content.push_str(&ktest_static_var);
    fs::write(&main_rs_path, main_rs_content).unwrap();

    // Build the kernel with the given base crate
    let target_name = get_current_crate_info().name;
    let default_bundle_directory = osdk_target_directory.join(target_name);
    let required_build_config = BuildConfig {
        manifest: config.manifest.clone(),
        cargo_args: config.cargo_args.clone(),
    };
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(&target_crate_dir).unwrap();
    let bundle = do_build(
        default_bundle_directory,
        &osdk_target_directory,
        &ws_target_directory,
        &required_build_config,
        &["--cfg ktest"],
    );
    std::env::remove_var("RUSTFLAGS");
    std::env::set_current_dir(original_dir).unwrap();

    let required_run_config = RunConfig {
        manifest: required_build_config.manifest.clone(),
        cargo_args: required_build_config.cargo_args.clone(),
        gdb_server_args: GdbServerArgs::default(),
    };

    bundle.run(&required_run_config);
}

fn get_workspace_default_members() -> Vec<String> {
    let metadata = get_cargo_metadata(None::<&str>, None::<&[&str]>).unwrap();
    let default_members = metadata
        .get("workspace_default_members")
        .unwrap()
        .as_array()
        .unwrap();
    default_members
        .iter()
        .map(|value| {
            // The default member is in the form of "<crate_name> <crate_version> (path+file://<crate_path>)"
            let default_member = value.as_str().unwrap();
            let path = default_member.split(' ').nth(2).unwrap();
            path.trim_start_matches("(path+file://")
                .trim_end_matches(')')
                .to_string()
        })
        .collect()
}
