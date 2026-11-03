# Build

Start by cloning the repository:

```sh
git clone https://github.com/baskerville/plato.git
cd plato
```

## Plato

#### Preliminary

Install the appropriate [compiler toolchain](https://drive.google.com/drive/folders/1YT6x2X070-cg_E8iWvNUUrWg5-t_YcV0) (the binaries of the `bin` directory need to be in your path).

Install the required dependencies: `wget`, `curl`, `git`, `pkg-config`, `unzip`, `jq`, `patchelf`.

Install *rustup*:
```sh
curl https://sh.rustup.rs -sSf | sh
```

Install the appropriate target:
```sh
rustup target add arm-unknown-linux-gnueabihf
```

#### Notes for MacOS

Install [homebrew](https://brew.sh/) if you haven't already and follow on screen instruction to add brew to your `PATH`. Then `brew install` the dependencies above (`curl` and `git` are usually already installed).

The compiler toolchain for Intel Macs can be found [here](https://www.dropbox.com/s/u4wtdik36f6mbqq/gcc-linaro-4.9.4-2017.01-20170615_darwin.tar.bz2?dl=1). MD5 checksum: `17f56603c9ceb8d6fc0bab87645fe430`

The above toolchain will also run on Apple Silicon Macs (M1/M2) using Rosetta, but you may want to get the native build [here](https://github.com/messense/homebrew-macos-cross-toolchains). Direct download link [here](https://github.com/messense/homebrew-macos-cross-toolchains/releases/download/v11.2.0/arm-unknown-linux-gnueabihf-aarch64-darwin.tar.gz).

For Catalina and later versions, sign the binaries to (hopefully) keep MacOS from blocking execution.

```sh
cd /full/path/to/extracted/toolchain/
find ./ -type f -perm +111 | xargs -n1 sudo codesign --force --deep --sign -
```

If you chose to install the Silicon Mac toolchain, create the following symlinks to compensate for a different naming scheme.

```sh
cd /full/path/to/extracted/toolchain/bin
ln -s arm-unknown-linux-gnueabihf-gcc arm-linux-gnueabihf-gcc
ln -s arm-unknown-linux-gnueabihf-ar arm-linux-gnueabihf-ar
ln -s arm-unknown-linux-gnueabihf-strip arm-linux-gnueabihf-strip
```

### Build Phase

```sh
./build.sh
```

If you are returning to an existing checkout, note that `build.sh` reuses
`libs/` when it already exists. It repairs missing linker symlinks, but does
not update the libraries themselves. The MuPDF headers in
`thirdparty/mupdf/include` and `libs/libmupdf.so` must match the `FZ_VERSION`
in `crates/core/src/document/mupdf_sys.rs` (currently 1.27.0). Older libraries
can cause undefined font-symbol errors at link time or fail at runtime.
Rebuild matching native dependencies with `./build.sh slow` after upgrading
them; keep a backup of any locally built libraries first.

### Distribution

```sh
./dist.sh
```

## Developer Tools

Install the required dependencies: *MuPDF 1.27.0*, *DjVuLibre*, *FreeType*, *HarfBuzz*.

### Emulator

Install one additional dependency: *SDL2*.

You can then run the emulator with:
```sh
./run-emulator.sh
```

### Importer

You can install the importer with:
```sh
./install-importer.sh
```
