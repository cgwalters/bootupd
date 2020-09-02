/*
 * Copyright (C) 2020 Red Hat, Inc.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use chrono::prelude::*;
use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The directory where updates are stored
pub(crate) const BOOTUPD_UPDATES_DIR: &str = "usr/lib/bootupd/updates";

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct ContentMetadata {
    /// The timestamp, which is used to determine update availability
    pub(crate) timestamp: NaiveDateTime,
    /// Human readable version number, like ostree it is not ever parsed, just displayed
    pub(crate) version: Option<String>,
}

/// Will be serialized into /boot/bootupd-state.json
#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct SavedState {
    pub(crate) installed: BTreeMap<String, ContentMetadata>,
}

// Should be stored in /usr/lib/bootupd/edges.json
//#[derive(Serialize, Deserialize, Debug)]
// #[serde(rename_all = "kebab-case")]
// pub(crate) struct UpgradeEdge {
//     /// Set to true if we should upgrade from an unknown state
//     #[serde(default)]
//     pub(crate) from_unknown: bool,
//     /// Upgrade from content past this timestamp
//     pub(crate) from_timestamp: Option<NaiveDateTime>,
// }
