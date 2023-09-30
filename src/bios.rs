use std::io::prelude::*;
use std::path::Path;
use std::process::Command;

use crate::component::*;
use crate::model::*;
use crate::packagesystem;
use anyhow::{bail, Result};

use crate::util;

// grub2-install file path
pub(crate) const GRUB_BIN: &str = "usr/sbin/grub2-install";

#[derive(Default)]
pub(crate) struct Bios {}

impl Bios {
    // get target device for running update
    fn get_device(&self) -> Result<String> {
        let mut cmd: Command;
        #[cfg(target_arch = "x86_64")]
        {
            // find /boot partition
            let boot_dir = Path::new("/").join("boot");
            cmd = Command::new("findmnt");
            cmd.arg("--noheadings")
                .arg("--output")
                .arg("SOURCE")
                .arg(boot_dir);
            let partition = util::cmd_output(&mut cmd)?;

            // lsblk to find parent device
            cmd = Command::new("lsblk");
            cmd.arg("--paths")
                .arg("--noheadings")
                .arg("--output")
                .arg("PKNAME")
                .arg(partition.trim());
        }

        #[cfg(target_arch = "powerpc64")]
        {
            // get PowerPC-PReP-boot partition
            cmd = Command::new("realpath");
            cmd.arg("/dev/disk/by-partlabel/PowerPC-PReP-boot");
        }

        let device = util::cmd_output(&mut cmd)?;
        Ok(device)
    }

    // Run grub2-install
    fn run_grub_install(&self, dest_root: &str, device: &str) -> Result<()> {
        let grub_install = Path::new("/").join(GRUB_BIN);
        if !grub_install.exists() {
            bail!("Failed to find {:?}", grub_install);
        }

        let mut cmd = Command::new(grub_install);
        let boot_dir = Path::new(dest_root).join("boot");
        // We forcibly inject mdraid1x because it's needed by CoreOS's default of "install raw disk image"
        // We also add part_gpt because in some cases probing of the partition map can fail such
        // as in a container, but we always use GPT.
        #[cfg(target_arch = "x86_64")]
        cmd.args(["--target", "i386-pc"])
            .args(["--boot-directory", boot_dir.to_str().unwrap()])
            .args(["--modules", "mdraid1x part_gpt"])
            .arg(device);

        #[cfg(target_arch = "powerpc64")]
        cmd.args(&["--target", "powerpc-ieee1275"])
            .args(&["--boot-directory", boot_dir.to_str().unwrap()])
            .arg("--no-nvram")
            .arg(device);

        let cmdout = cmd.output()?;
        if !cmdout.status.success() {
            std::io::stderr().write_all(&cmdout.stderr)?;
            bail!("Failed to run {:?}", cmd);
        }
        Ok(())
    }
}

impl Component for Bios {
    fn name(&self) -> &'static str {
        "BIOS"
    }

    fn install(
        &self,
        src_root: &openat::Dir,
        dest_root: &str,
        device: &str,
    ) -> Result<InstalledContent> {
        let meta = if let Some(meta) = get_component_update(src_root, self)? {
            meta
        } else {
            anyhow::bail!("No update metadata for component {} found", self.name());
        };

        self.run_grub_install(dest_root, device)?;
        Ok(InstalledContent {
            meta,
            filetree: None,
            adopted_from: None,
        })
    }

    fn generate_update_metadata(&self, sysroot_path: &str) -> Result<ContentMetadata> {
        let grub_install = Path::new(sysroot_path).join(GRUB_BIN);
        if !grub_install.exists() {
            bail!("Failed to find {:?}", grub_install);
        }

        // Query the rpm database and list the package and build times for /usr/sbin/grub2-install
        let meta = packagesystem::query_files(sysroot_path, [&grub_install])?;
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
        todo!();
    }

    fn query_update(&self, sysroot: &openat::Dir) -> Result<Option<ContentMetadata>> {
        get_component_update(sysroot, self)
    }

    fn run_update(&self, sysroot: &openat::Dir, _: &InstalledContent) -> Result<InstalledContent> {
        let updatemeta = self.query_update(sysroot)?.expect("update available");
        let device = self.get_device()?;
        let device = device.trim();
        self.run_grub_install("/", device)?;

        let adopted_from = None;
        Ok(InstalledContent {
            meta: updatemeta,
            filetree: None,
            adopted_from,
        })
    }

    fn validate(&self, _: &InstalledContent) -> Result<ValidationResult> {
        Ok(ValidationResult::Skip)
    }
}
