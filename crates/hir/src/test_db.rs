/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

//! Database used for testing `hir`.

use std::fmt;
use std::panic;
use std::sync::Arc;

use elp_base_db::salsa;
use elp_base_db::FileId;
use elp_base_db::FileLoader;
use elp_base_db::FileLoaderDelegate;
use elp_base_db::SourceDatabase;
use elp_base_db::Upcast;

use crate::db::MinInternDatabase;

#[salsa::database(
    elp_base_db::SourceDatabaseExtStorage,
    elp_base_db::SourceDatabaseStorage,
    crate::db::MinDefDatabaseStorage,
    crate::db::MinInternDatabaseStorage
)]
#[derive(Default)]
pub(crate) struct TestDB {
    storage: salsa::Storage<TestDB>,
}

impl Upcast<dyn SourceDatabase> for TestDB {
    fn upcast(&self) -> &(dyn SourceDatabase + 'static) {
        self
    }
}

impl Upcast<dyn MinInternDatabase> for TestDB {
    fn upcast(&self) -> &(dyn MinInternDatabase + 'static) {
        self
    }
}

impl salsa::Database for TestDB {}

impl fmt::Debug for TestDB {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TestDB").finish()
    }
}

impl panic::RefUnwindSafe for TestDB {}

impl FileLoader for TestDB {
    fn file_text(&self, file_id: FileId) -> Arc<String> {
        FileLoaderDelegate(self).file_text(file_id)
    }
}
