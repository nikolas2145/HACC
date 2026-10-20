use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
fn visual_studio_environment(out_dir: &Path) -> Vec<(String, String)> {
    let program_files =
        env::var_os("ProgramFiles(x86)").expect("ProgramFiles(x86) is not set on Windows");
    let vswhere =
        PathBuf::from(program_files).join("Microsoft Visual Studio/Installer/vswhere.exe");
    let found = Command::new(&vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .expect("Visual Studio locator (vswhere.exe) is missing");
    assert!(found.status.success(), "vswhere failed");
    let installation = String::from_utf8(found.stdout).expect("Invalid Visual Studio path");
    let devcmd = Path::new(installation.trim()).join("Common7/Tools/VsDevCmd.bat");
    assert!(devcmd.exists(), "Visual Studio C++ build tools are missing");
    let script = out_dir.join("setup_msvc.bat");
    std::fs::write(
        &script,
        format!(
            "@echo off\r\ncall \"{}\" -no_logo -arch=x64 -host_arch=x64 >nul\r\nif errorlevel 1 exit /b 1\r\nset\r\n",
            devcmd.display()
        ),
    )
    .expect("Could not write Visual Studio setup script");
    let output = Command::new("cmd")
        .arg("/C")
        .arg(&script)
        .output()
        .expect("Could not initialize Visual Studio build environment");
    assert!(
        output.status.success(),
        "VsDevCmd.bat failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| !key.is_empty())
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn main() {
    println!("cargo:rerun-if-changed=native/gpu_codec.cu");
    println!("cargo:rerun-if-changed=native/gpu_codec.h");
    if env::var_os("CARGO_FEATURE_GPU").is_none() {
        return;
    }

    let cuda = env::var_os("CUDA_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            let root = Path::new(r"C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA");
            std::fs::read_dir(root)
                .ok()?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join("bin/nvcc.exe").exists())
                .max()
        })
        .expect(
            "GPU build requires CUDA Toolkit (nvcc). Set CUDA_PATH to its installation directory.",
        );
    let nvcc = cuda.join(if cfg!(windows) {
        "bin/nvcc.exe"
    } else {
        "bin/nvcc"
    });
    if !nvcc.exists() {
        panic!("CUDA Toolkit compiler missing: {}", nvcc.display());
    }
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let library = out_dir.join(if cfg!(windows) {
        "gpu_codec.lib"
    } else {
        "libgpu_codec.a"
    });
    let mut compiler = Command::new(&nvcc);
    #[cfg(windows)]
    compiler.envs(visual_studio_environment(&out_dir));
    let status = compiler
        .args([
            "--lib",
            "--std=c++14",
            "-DGPU_CODEC_STATIC",
            "-gencode=arch=compute_70,code=compute_70",
            "-gencode=arch=compute_86,code=sm_86",
            "-o",
        ])
        .arg(&library)
        .arg("native/gpu_codec.cu")
        .status()
        .expect("could not start nvcc");
    assert!(status.success(), "nvcc failed compiling GPU codec");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!(
        "cargo:rustc-link-search=native={}",
        cuda.join(if cfg!(windows) { "lib/x64" } else { "lib64" })
            .display()
    );
    println!("cargo:rustc-link-lib=static=gpu_codec");
    println!("cargo:rustc-link-lib=static=cudart_static");
}
