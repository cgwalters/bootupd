/*
 * Copyright (C) 2020 Red Hat, Inc.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::Result;
use std::fs::File;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

use crate::model::*;

/// A component along with a possible update
#[typetag::serde(tag = "type")]
pub(crate) trait Component {
    fn name(&self) -> &'static str;

    fn install(&self, src_root: &str, dest_root: &str) -> Result<ContentMetadata>;

    fn generate_update_metadata(&self, sysroot: &str) -> Result<ContentMetadata>;

    fn query_update(&self) -> Result<Option<ContentMetadata>>;

    fn run_update(&self) -> Result<()>;
}

pub(crate) fn component_update_metapath(sysroot: &str, component: &dyn Component) -> PathBuf {
    Path::new(sysroot)
        .join(BOOTUPD_UPDATES_DIR)
        .join(format!("{}.json", component.name()))
        .to_path_buf()
}

pub(crate) fn component_updatedir(sysroot: &str, component: &dyn Component) -> PathBuf {
    Path::new(sysroot)
        .join(BOOTUPD_UPDATES_DIR)
        .join(component.name())
        .to_path_buf()
}

/// Helper method for writing an update file
pub(crate) fn write_update_metadata(
    sysroot: &str,
    component: &dyn Component,
    meta: &ContentMetadata,
) -> Result<()> {
    let metap = component_update_metapath(sysroot, component);
    let mut f = std::io::BufWriter::new(std::fs::File::create(&metap)?);
    serde_json::to_writer(&mut f, &meta)?;
    f.flush()?;
    Ok(())
}

pub(crate) fn get_component_update(
    sysroot: &str,
    component: &dyn Component,
) -> Result<Option<ContentMetadata>> {
    let metap = component_update_metapath(sysroot, component);
    if !metap.exists() {
        return Ok(None);
    }
    let mut f = std::io::BufReader::new(File::open(&metap)?);
    let u = serde_json::from_reader(&mut f)?;
    Ok(Some(u))
}
