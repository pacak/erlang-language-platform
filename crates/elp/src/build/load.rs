/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

//! Loads a rebar project into a static instance of ELP,
//! without support for incorporating changes
use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use crossbeam_channel::unbounded;
use crossbeam_channel::Receiver;
use elp_ide::elp_ide_db::elp_base_db::loader;
use elp_ide::elp_ide_db::elp_base_db::loader::Handle;
use elp_ide::elp_ide_db::elp_base_db::AbsPathBuf;
use elp_ide::elp_ide_db::elp_base_db::FileSetConfig;
use elp_ide::elp_ide_db::elp_base_db::IncludeOtp;
use elp_ide::elp_ide_db::elp_base_db::ProjectApps;
use elp_ide::elp_ide_db::elp_base_db::ProjectId;
use elp_ide::elp_ide_db::elp_base_db::SourceDatabase;
use elp_ide::elp_ide_db::elp_base_db::SourceDatabaseExt;
use elp_ide::elp_ide_db::elp_base_db::SourceRoot;
use elp_ide::elp_ide_db::elp_base_db::SourceRootId;
use elp_ide::elp_ide_db::elp_base_db::Vfs;
use elp_ide::AnalysisHost;
use elp_project_model::DiscoverConfig;
use elp_project_model::Project;
use elp_project_model::ProjectManifest;

use crate::build::types::LoadResult;
use crate::cli::Cli;
use crate::reload::ProjectFolders;

pub fn load_project_at(
    cli: &dyn Cli,
    root: &Path,
    conf: DiscoverConfig,
    include_otp: IncludeOtp,
) -> Result<LoadResult> {
    let root = fs::canonicalize(root)?;
    let root = AbsPathBuf::assert(root);
    let manifest = ProjectManifest::discover_single(&root, &conf)?;

    log::info!("Discovered project: {:?}", manifest);
    let pb = cli.spinner("Loading build info");
    let project = Project::load(manifest)?;
    pb.finish();

    load_project(cli, project, include_otp)
}

fn load_project(cli: &dyn Cli, project: Project, include_otp: IncludeOtp) -> Result<LoadResult> {
    let project_id = ProjectId(0);
    let (sender, receiver) = unbounded();
    let mut vfs = Vfs::default();
    let mut loader = {
        let loader =
            vfs_notify::NotifyHandle::spawn(Box::new(move |msg| sender.send(msg).unwrap()));
        Box::new(loader)
    };

    let projects = [project.clone()];
    let project_apps = ProjectApps::new(&projects, include_otp);
    let folders = ProjectFolders::new(&project_apps);

    let vfs_loader_config = loader::Config {
        load: folders.load,
        watch: vec![],
        version: 0,
    };
    loader.set_config(vfs_loader_config);

    let analysis_host = load_database(
        cli,
        &project_apps,
        &folders.file_set_config,
        &mut vfs,
        &receiver,
    )?;
    Ok(LoadResult::new(
        analysis_host,
        vfs,
        project_id,
        project,
        folders.file_set_config,
    ))
}

fn load_database(
    cli: &dyn Cli,
    project_apps: &ProjectApps,
    file_set_config: &FileSetConfig,
    vfs: &mut Vfs,
    receiver: &Receiver<loader::Message>,
) -> Result<AnalysisHost> {
    let mut analysis_host = AnalysisHost::default();
    let db = analysis_host.raw_database_mut();

    let pb = cli.progress(0, "Loading applications");

    for task in receiver {
        match task {
            loader::Message::Progress {
                n_done, n_total, ..
            } => {
                pb.set_length(n_total as u64);
                pb.set_position(n_done as u64);
                if n_done == n_total {
                    break;
                }
            }
            loader::Message::Loaded { files } => {
                for (path, contents) in files {
                    vfs.set_file_contents(path.into(), contents);
                }
            }
        }
    }

    pb.finish();

    let pb = cli.spinner("Seeding database");

    let sets = file_set_config.partition(vfs);
    for (idx, set) in sets.into_iter().enumerate() {
        let root_id = SourceRootId(idx as u32);
        for file_id in set.iter() {
            db.set_file_source_root(file_id, root_id);
        }
        let root = SourceRoot::new(set);
        db.set_source_root(root_id, Arc::new(root));
    }

    project_apps.app_structure().apply(db);

    let project_id = ProjectId(0);
    db.ensure_erlang_service(project_id)?;
    let changes = vfs.take_changes();
    for file in changes {
        if file.exists() {
            let contents = vfs.file_contents(file.file_id).to_vec();
            match String::from_utf8(contents) {
                Ok(text) => {
                    db.set_file_text(file.file_id, Arc::new(text));
                }
                Err(err) => {
                    // Fall back to lossy latin1 loading of files.
                    // This should only affect files from yaws, and
                    // possibly OTP that are latin1 encoded.
                    let contents = err.into_bytes();
                    let text = contents.into_iter().map(|byte| byte as char).collect();
                    db.set_file_text(file.file_id, Arc::new(text));
                }
            }
        }
    }

    pb.finish();

    Ok(analysis_host)
}
