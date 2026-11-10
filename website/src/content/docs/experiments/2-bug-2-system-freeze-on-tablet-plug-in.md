---
title: "2 Bug 2 - System Freeze on Tablet Plug-in"
sidebar:
  order: 2
  label: "2 Bug 2 - System Freeze on Tablet Plug-in"
---

Plugging in the Android tablet (Samsung Galaxy Tab S9 FE, `SM-X616B`) via USB causes the entire GNOME/Wayland desktop to hard-freeze, requiring a physical power cycle.

| Field | Value |
| :-- | :-- |
| **ID** | 2 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

### Problem Description

Plugging in the Android tablet (Samsung Galaxy Tab S9 FE, `SM-X616B`) via USB causes the entire GNOME/Wayland desktop to hard-freeze, requiring a physical power cycle.

### Hardware and Environment Baseline

- **Laptop SoC / CPU**: Intel Core Ultra (Meteor Lake-P)
- **Primary GPU**: Intel Arc Graphics [8086:7d55] (rev 08), Subsystem [103c:8cac]
- **Primary Kernel Driver**: `i915` (45 references, managing `/dev/dri/card1` and `/dev/dri/renderD128`)
- **Kernel Version**: `Linux user 7.0.0-42-generic #42~24.04.1-Ubuntu SMP PREEMPT_DYNAMIC`
- **Compositor**: GNOME Mutter Wayland (`gnome-shell 46.0`)
- **Secondary Display Module**: EVDI (`evdi.ko.zst` via DKMS, version 1.15.0)

### Raw System Inspection Logs



### PCI Device & Driver

```bash
$ lspci -nnk | grep -A3 -i vga
00:02.0 VGA compatible controller [0300]: Intel Corporation Meteor Lake-P [Intel Arc Graphics] [8086:7d55] (rev 08)
	DeviceName:  Onboard IGD
	Subsystem: Hewlett-Packard Company Meteor Lake-P [Intel Arc Graphics] [103c:8cac]
	Kernel driver in use: i915
```

### Kernel Modules Involving DRM and EVDI

```bash
$ lsmod | grep -E "i915|xe|evdi|drm"
xe                   4378624  0
drm_gpusvm_helper      61440  1 xe
gpu_sched              69632  1 xe
drm_gpuvm              53248  1 xe
drm_exec               12288  2 drm_gpuvm,xe
drm_suballoc_helper    24576  1 xe
i915                 5025792  45
drm_buddy              28672  2 xe,i915
drm_display_helper    299008  2 xe,i915
cec                    98304  3 drm_display_helper,xe,i915
i2c_algo_bit           16384  2 xe,i915
intel_vsec             24576  3 intel_pmc_ssram_telemetry,pmt_telemetry,xe
evdi                   90112  2
drm_ttm_helper         20480  2 evdi,xe
ttm                   135168  3 drm_ttm_helper,xe,i915
video                  77824  2 xe,i915
```

### Module Parameters for EVDI

```bash
$ tail -n +1 /sys/module/evdi/parameters/*
==> /sys/module/evdi/parameters/initial_device_count <==
2
```

### Existing Modprobe Configurations

```bash
$ cat /etc/modprobe.d/uscreen-evdi.conf
options evdi initial_device_count=2
```

### Root Cause Analysis

1. **DRM Subsystem Lock Inversion Race**:
   On Intel Meteor Lake-P systems running the `i915` driver, the Direct Rendering Manager (DRM) subsystem maintains global and per-device mutexes (`drm_device.mode_config.mutex` and `dma_resv` reservation locks).
   When the Android tablet is plugged into USB, ADB daemon triggers connection detection. If EVDI attempts to register or dynamically instantiate a DRM device node (`/sys/devices/evdi/add` or during asynchronous driver loading) without strict module dependency ordering:
   - EVDI acquires its internal `painter` lock and calls into `drm_kms_helper` to register virtual connectors.
   - Concurrently, the primary Intel graphics driver (`i915`) is handling KMS display pipe commits or power state management.
   - If EVDI is loaded concurrently or before `i915`/`drm_kms_helper` has completed subsystem setup, an AB-BA deadlock occurs between the DRM subsystem mode configuration lock and the driver locks.
   - Because GNOME Shell (Mutter) runs a single native KMS thread for all DRM devices, a deadlock in any KMS ioctl stalls the entire compositor rendering loop, freezing all input and video output on the laptop screen.

2. **Missing `softdep` Directive**:
   The current configuration in `/etc/modprobe.d/uscreen-evdi.conf` specifies:
   `options evdi initial_device_count=2`
   However, it **completely omits** the software dependency rule:
   `softdep evdi pre: drm_kms_helper i915`
   Without `softdep`, `systemd-modules-load` or udev hotplug can load `evdi` prior to or in parallel with `i915`, triggering the race condition upon device connection.

3. **Missing EVDI Udev Permission Rule**:
   Stock systems do not tag EVDI DRM card device nodes with `TAG+="uaccess"`. As a result, when a new EVDI card is hotplugged, `systemd-logind` does not immediately attach user ACLs (`user:user:rw-`). This can cause access race conditions between Mutter's unprivileged user process and the device node.


## Result

### Remediation Plan

1. Configure `/etc/modprobe.d/evdi.conf` (and update `/etc/modprobe.d/uscreen-evdi.conf` and repository templates `packaging/uscreen-evdi.conf`) to include:
   ```ini
   options evdi initial_device_count=2
   softdep evdi pre: drm_kms_helper i915
   ```
2. Create udev rule `/etc/udev/rules.d/99-uscreen-evdi.rules` (and repository file `packaging/99-uscreen-evdi.rules`):
   ```udev
   ACTION=="add", SUBSYSTEM=="drm", KERNEL=="card[0-9]*", DRIVERS=="evdi", TAG+="uaccess"
   ```
3. Update `scripts/install.sh` and `Makefile` so new installations automatically deploy the `softdep` line and udev rule.

### Verification

- Validate module dependencies with `modinfo -F softdep evdi`.
- Inspect `/dev/dri/card*` permissions to verify proper `uaccess` ACL assignment.


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/02_bug2_evdi_intel_freeze.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/02_bug2_evdi_intel_freeze.md)
