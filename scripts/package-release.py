#!/usr/bin/env python3
"""Build and stage offline desktop artifacts from the committed Cargo.lock."""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import struct
import subprocess
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parent.parent
NAME = "plan-my-cabinet"
APP = "Plan My Cabinet"
APP_ID = "org.planmycabinet.PlanMyCabinet"
TARGETS = {"macos-arm64": "aarch64-apple-darwin", "linux-x86_64": "x86_64-unknown-linux-gnu"}
RUNTIME = """Plan My Cabinet desktop runtime prerequisites

macOS arm64: a Metal-capable GPU/driver and an interactive desktop session.
Linux x86_64: a working Vulkan GPU/driver, X11 or Wayland desktop session,
window-system libraries (including X11/Wayland/XKB) and a working XDG Desktop
Portal backend for native file dialogs. Distribution-specific dynamic libraries
must be present; inspect the executable with ldd on the target distribution.
No account or network connection is needed for core use.

The macOS bundle has only an ad-hoc signature, with no Developer ID signature
or notarization. Gatekeeper may block launch;
installers should review the source and artifact before granting an exception.
Linux archives are not signed or integrated with a package manager.
No minimum OS release, GPU model, driver version or Linux distribution has
been certified by these build steps. See docs/release-checklist.md in source.
"""


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True)


def dependency_packages(target):
    metadata = json.loads(run("cargo", "metadata", "--locked", "--offline", "--format-version", "1", "--filter-platform", target))
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    # Only the application and its normal/build dependency closure; dev-only
    # packages in Cargo.lock are not shipped in the executable.
    visited = set()
    pending = [metadata["resolve"]["root"]]
    while pending:
        package_id = pending.pop()
        if package_id in visited:
            continue
        visited.add(package_id)
        for dep in nodes[package_id]["deps"]:
            if any(kind["kind"] != "dev" for kind in dep["dep_kinds"]):
                pending.append(dep["pkg"])
    return sorted((packages[i] for i in visited if i != metadata["resolve"]["root"]),
                  key=lambda p: (p["name"], p["version"], p["source"] or ""))


LICENSE_NAME = re.compile(r"^(?:LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT|UNLICENSE)(?:[._ -].*)?$", re.I)
SAFE_COMPONENT = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._+-]*$")


def license_sources(package):
    """Find source-shipped notices, never following a link out of the crate root."""
    root = Path(package["manifest_path"]).parent.resolve(strict=True)
    candidates = {p for p in root.iterdir() if LICENSE_NAME.match(p.name)}
    # Cargo's license-file may use a nonstandard filename or a subdirectory.
    if package.get("license_file"):
        candidates.add(root / package["license_file"])
    files = []
    for path in sorted(candidates, key=lambda p: p.as_posix()):
        if not path.is_file() or not path.resolve().is_relative_to(root):
            raise RuntimeError(f"invalid license file for {package['name']}: {path}")
        relative = path.relative_to(root)
        if any(not SAFE_COMPONENT.fullmatch(part) or part in (".", "..") for part in relative.parts):
            raise RuntimeError(f"unsafe license path for {package['name']}: {relative}")
        contents = path.read_bytes()
        if not contents.strip():
            raise RuntimeError(f"empty license file for {package['name']}: {relative}")
        files.append((relative, contents))
    return files


def dependency_notices(directory, target):
    lines = ["Rust dependency license inventory",
             "Generated from Cargo.lock with cargo metadata --locked --offline",
             f"Target: {target}", "Normal/build dependency closure only; dev-only crates excluded.",
             "Expressions are upstream metadata; copied files are source-shipped texts,",
             "not an assertion that every license in an expression is represented.",
             "METADATA-ONLY means no standalone license/notice file was shipped at the",
             "crate root (or named by license_file). Consult upstream for full terms.", ""]
    written = {}
    for package in dependency_packages(target):
        name, version = package["name"], package["version"]
        if not all(SAFE_COMPONENT.fullmatch(s) and s not in (".", "..") for s in (name, version)):
            raise RuntimeError(f"unsafe crate name/version: {name} {version}")
        expression = package.get("license")
        if not expression and not package.get("license_file"):
            raise RuntimeError(f"undeclared dependency license: {name} {version}")
        files = license_sources(package)
        if not files:
            if not expression:
                raise RuntimeError(f"no license text or expression: {name} {version}")
            lines.append(f"{name} {version} | {expression} | {package['source'] or 'local'} | METADATA-ONLY (no source-shipped text)")
            continue
        paths = []
        for relative, contents in files:
            destination = Path("dependencies") / f"{name}-{version}" / relative
            key = destination.as_posix()
            if key in written and written[key] != contents:
                raise RuntimeError(f"conflicting dependency license text: {key}")
            if key not in written:
                output = directory / destination
                output.parent.mkdir(parents=True, exist_ok=True)
                output.write_bytes(contents)
                written[key] = contents
            paths.append(key)
        lines.append(f"{name} {version} | {expression or 'license_file'} | {package['source'] or 'local'} | {', '.join(paths)}")
    write(directory / "rust-dependencies.txt", "\n".join(lines) + "\n")


def write(path, contents):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents, encoding="utf-8")


def font_notices(directory):
    """Inventory the six embedded static faces and ship both upstream OFL texts."""
    directory.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / "assets/fonts/OFL.txt", directory / "NotoSans-OFL.txt")
    shutil.copyfile(ROOT / "assets/fonts/JetBrainsMono-OFL.txt", directory / "JetBrainsMono-OFL.txt")
    lines = ["Embedded font inventory | SIL Open Font License 1.1",
             "Static upstream faces; no runtime font downloads.",
             "File | Weight | SHA-256 | License", ""]
    for family in ("NotoSans", "JetBrainsMono"):
        for weight, value in (("Regular", 400), ("Medium", 500), ("SemiBold", 600)):
            filename = f"{family}-{weight}.ttf"
            data = (ROOT / "assets/fonts" / filename).read_bytes()
            if not data:
                raise RuntimeError(f"empty bundled font: {filename}")
            lines.append(f"{filename} | {value} | {hashlib.sha256(data).hexdigest()} | {family}-OFL.txt")
    write(directory / "fonts.txt", "\n".join(lines) + "\n")
    shutil.copyfile(ROOT / "docs/redesign-typography.md", directory / "redesign-typography.md")


def notices(directory, target):
    font_notices(directory)
    shutil.copyfile(ROOT / "docs/hinge-source-review.md", directory / "hinge-source-review.md")
    dependency_notices(directory, target)
    write(directory / "RUNTIME.txt", RUNTIME)
    write(directory / "CATALOG.txt", "Reviewed factual catalog data compiled into the program:\nFGVTN Click 3D Slow Reta / Calço 0 kit 51MX153DRV00100; plate 52MX15FG11003D.\nSource: FGVTN General Catalog, May 2025, printed page 23 (PDF page 14), modified 2026-09-16.\nSource URL: https://www.fgvtn.com.br/site/novopdf/Catalogo_Geral.pdf\nSource SHA-256: e8aafa4f3656a108e8e91dd1681685455a4bf4f644cf01d4ecfa3fd80bf44df2\nSee the bundled hinge-source-review.md for the field-to-source mapping. Manufacturer artwork and PDF are not included. Projects pin their own catalog snapshots.\n")


def check_binary(binary, target):
    data = binary.read_bytes()[:64]
    if target == "aarch64-apple-darwin":
        if len(data) < 12 or data[:4] != b"\xcf\xfa\xed\xfe" or struct.unpack_from("<I", data, 4)[0] != 0x0100000C:
            raise RuntimeError(f"expected arm64 Mach-O: {binary}")
    elif len(data) < 20 or data[:4] != b"\x7fELF" or data[5] != 1 or struct.unpack_from("<H", data, 18)[0] != 62:
        raise RuntimeError(f"expected little-endian x86_64 ELF: {binary}")


def mac_app(stage, binary, version, target):
    bundle = stage / f"{APP}.app"
    app = bundle / "Contents"
    (app / "MacOS").mkdir(parents=True)
    shutil.copyfile(binary, app / "MacOS" / NAME)
    (app / "MacOS" / NAME).chmod(0o755)
    with (app / "Info.plist").open("wb") as file:
        plistlib.dump({"CFBundleDevelopmentRegion": "en", "CFBundleExecutable": NAME,
                       "CFBundleIdentifier": APP_ID, "CFBundleInfoDictionaryVersion": "6.0",
                       "CFBundleName": APP, "CFBundlePackageType": "APPL",
                       "CFBundleShortVersionString": version, "CFBundleVersion": version,
                       "NSHighResolutionCapable": True}, file, sort_keys=True)
    notices(app / "Resources" / "Licenses", target)
    # The Rust linker ad-hoc signs the Mach-O alone. Seal the full bundle after
    # adding resources, without implying Developer ID signing/notarization.
    subprocess.run(["codesign", "--force", "--sign", "-", str(bundle)], check=True)


def linux_archive(stage, binary, version, target, output, epoch):
    folder = stage / f"{NAME}-{version}-linux-x86_64"
    exe = folder / "bin" / NAME
    exe.parent.mkdir(parents=True)
    shutil.copyfile(binary, exe)
    exe.chmod(0o755)
    write(folder / "share/applications" / f"{APP_ID}.desktop", f"[Desktop Entry]\nType=Application\nName={APP}\nComment=Offline woodworking design and cut planning\nExec={NAME}\nTerminal=false\nCategories=Graphics;Engineering;\n")
    write(folder / "share/metainfo" / f"{APP_ID}.metainfo.xml", f'<?xml version="1.0" encoding="UTF-8"?>\n<component type="desktop-application"><id>{APP_ID}</id><name>{APP}</name><summary>Offline woodworking design and cut planning</summary><launchable type="desktop-id">{APP_ID}.desktop</launchable></component>\n')
    notices(folder / "share/doc" / NAME, target)
    write(folder / "README.txt", "Extract the archive, run bin/plan-my-cabinet in a desktop session. To register the launcher, install bin/ into PATH and share/ into the matching share prefix (e.g. /usr/local). Read share/doc/plan-my-cabinet/RUNTIME.txt for prerequisites and unsigned-build limitations.\n")
    with output.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch, compresslevel=9) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as tar:
                for path in [folder, *sorted(folder.rglob("*"))]:
                    relative = path.relative_to(stage).as_posix()
                    info = tar.gettarinfo(str(path), arcname=relative)
                    info.mtime = epoch
                    info.uid = info.gid = 0
                    info.uname = info.gname = ""
                    if path.is_file():
                        with path.open("rb") as data:
                            tar.addfile(info, data)
                    else:
                        tar.addfile(info)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=TARGETS)
    parser.add_argument("--out-dir", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    target = TARGETS[args.platform]
    host = platform.system(), platform.machine()
    expected = ("Darwin", "arm64") if args.platform == "macos-arm64" else ("Linux", "x86_64")
    if host != expected:
        installed = run("rustup", "target", "list", "--installed").splitlines()
        if target not in installed:
            parser.error(f"{target} is not installed (host: {host}); no cross-build artifact assembled")
    manifest = json.loads(run("cargo", "metadata", "--locked", "--offline", "--format-version", "1", "--no-deps"))
    version = next(p["version"] for p in manifest["packages"] if p["name"] == NAME)
    subprocess.run(["cargo", "build", "--locked", "--offline", "--release", "--target", target, "--bin", NAME], cwd=ROOT, check=True)
    binary = Path(manifest["target_directory"]) / target / "release" / NAME
    check_binary(binary, target)
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    output_dir = args.out_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="package-", dir=output_dir) as temp:
        stage = Path(temp)
        if args.platform == "macos-arm64":
            mac_app(stage, binary, version, target)
            destination = output_dir / f"{APP}.app"
            if destination.exists():
                shutil.rmtree(destination)
            shutil.move(str(stage / f"{APP}.app"), destination)
        else:
            destination = output_dir / f"{NAME}-{version}-linux-x86_64.tar.gz"
            assembled = stage / destination.name
            linux_archive(stage, binary, version, target, assembled, epoch)
            os.replace(assembled, destination)
    print(destination)


if __name__ == "__main__":
    main()
