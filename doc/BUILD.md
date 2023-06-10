# Build

Start by cloning the repository:

```sh
git clone https://github.com/baskerville/plato.git
cd plato
```

There are two ways to build *Plato*:
- [Local *Rust* Setup](#local)
- [With *Docker*/*Podman*](#docker)

## Local

### Plato

#### Preliminary

Install the appropriate [compiler toolchain](https://github.com/kobolabs/Kobo-Reader/tree/master/toolchain) (the binaries of the `bin` directory need to be in your path).

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

### Distribution

```sh
./dist.sh
```

## Developer Tools

Install the required dependencies: *MuPDF 1.22.1*, *DjVuLibre*, *FreeType*, *HarfBuzz*.

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

## Docker

### Plato

1. Build the image for armhf: `docker build . -t plato:armhf`
2. The following compiles, mounts a local volume, and outputs the `plato` binary to your local folder `target/arm-unknown-linux-gnueabihf`:

```sh
docker run --rm -t -v $(pwd)/target:/plato/target plato:armhf
```

You can copy the binary to your Kobo device (make sure you install an existing release first) and it will run.

### Developer Tools

1. Build the image for dev environments: `docker build . -f Dockerfile.dev -t plato:dev`
2. The following runs tests, compiles, mounts a local volume, and outputs all binaries to your local folder `target/debug`:

```sh
docker run --rm -t -v $(pwd):/plato plato:dev
```

If the emulator or importer fail to run, please follow the steps in [Local–Developer Tools](#developer-tools) to ensure you have the relevant libraries.
