use std::os::fd::AsRawFd;
use std::path::Path;

use crate::component::*;
use crate::filetree;
use crate::model::*;
use anyhow::Context;
use anyhow::Result;

const BOOT: &str = "boot";
const STATIC_DIR: &str = "/usr/lib/bootupd/grub2-static";
const STATIC_CONFIG_NAME: &str = "grub.cfg";

#[derive(Default)]
pub(crate) struct GrubConfigs {}

impl GrubConfigs {
    fn open_boot(&self) -> Result<openat::Dir> {
        openat::Dir::open("/boot").context("opening boot dir")
    }
}

impl Component for GrubConfigs {
    fn name(&self) -> &'static str {
        "grub-configs"
    }

    fn install(
        &self,
        src_root: &openat::Dir,
        dest_root: &str,
        _: &str,
    ) -> Result<InstalledContent> {
        let meta = if let Some(meta) = get_component_update(src_root, self)? {
            meta
        } else {
            anyhow::bail!("No update metadata for component {} found", self.name());
        };
        log::debug!("Found metadata {}", meta.version);
        let srcdir_name = component_updatedirname(self);
        let ft = crate::filetree::FileTree::new_from_dir(&src_root.sub_dir(&srcdir_name)?)?;
        let destdir = &Path::new(dest_root).join(BOOT);
        // TODO - add some sort of API that allows directly setting the working
        // directory to a file descriptor.
        let r = std::process::Command::new("cp")
            .args(["-rp", "--reflink=auto"])
            .arg(&srcdir_name)
            .arg(destdir)
            .current_dir(format!("/proc/self/fd/{}", src_root.as_raw_fd()))
            .status()?;
        if !r.success() {
            anyhow::bail!("Failed to copy: {r:?}");
        }
        Ok(InstalledContent {
            meta,
            filetree: Some(ft),
            adopted_from: None,
        })
    }

    fn generate_update_metadata(&self, sysroot_path: &str) -> Result<ContentMetadata> {
        let config_path = &Path::new(STATIC_DIR).join(STATIC_CONFIG_NAME);

        let meta = crate::packagesystem::query_files(sysroot_path, [config_path])?;
        write_update_metadata(sysroot_path, self, &meta)?;
        Ok(meta)
    }

    fn query_adopt(&self) -> Result<Option<Adoptable>> {
        Ok(None)
    }

    #[allow(unused_variables)]
    fn adopt_update(
        &self,
        sysroot: &openat::Dir,
        update: &ContentMetadata,
    ) -> Result<InstalledContent> {
        unreachable!()
    }

    fn query_update(&self, sysroot: &openat::Dir) -> Result<Option<ContentMetadata>> {
        get_component_update(sysroot, self)
    }

    fn run_update(
        &self,
        sysroot: &openat::Dir,
        current: &InstalledContent,
    ) -> Result<InstalledContent> {
        let currentf = current
            .filetree
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No filetree for grub configs found!"))?;
        let updatemeta = self.query_update(sysroot)?.expect("update available");
        let updated = sysroot
            .sub_dir(&component_updatedirname(self))
            .context("opening update dir")?;
        let updatef = filetree::FileTree::new_from_dir(&updated).context("reading update dir")?;
        let diff = currentf.diff(&updatef)?;
        let destdir = self.open_boot()?;
        log::trace!("applying diff: {}", &diff);
        filetree::apply_diff(&updated, &destdir, &diff, None)
            .context("applying filesystem changes")?;
        let adopted_from = None;
        Ok(InstalledContent {
            meta: updatemeta,
            filetree: Some(updatef),
            adopted_from,
        })
    }

    fn validate(&self, current: &InstalledContent) -> Result<ValidationResult> {
        let currentf = current
            .filetree
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No filetree for grub configs found!"))?;
        let bootdir = self.open_boot()?;
        let diff = currentf.relative_diff_to(&bootdir)?;
        let mut errs = Vec::new();
        for f in diff.changes.iter() {
            errs.push(format!("Changed: {}", f));
        }
        for f in diff.removals.iter() {
            errs.push(format!("Removed: {}", f));
        }
        assert_eq!(diff.additions.len(), 0);
        if !errs.is_empty() {
            Ok(ValidationResult::Errors(errs))
        } else {
            Ok(ValidationResult::Valid)
        }
    }
}
