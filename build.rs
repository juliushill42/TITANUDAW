fn main() {
    cc::Build::new()
        .file("src/audio_engine.c")
        .flag("-O3")
        .flag("-march=native")
        .flag("-ffast-math")
        .flag("-pthread")
        .compile("titan_audio");

    println!("cargo:rustc-link-lib=asound");
    println!("cargo:rustc-link-lib=pthread");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rerun-if-changed=src/audio_engine.c");
    println!("cargo:rerun-if-changed=src/audio_engine.h");
}
