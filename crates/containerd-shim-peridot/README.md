## containerd-shim-peridot

This is a [containerd] shim for running WebAssembly modules using Peridot. This shim is heavily
based on the [Wasmtime] containerd shim from [runwasi].

Components are not supported yet: the shim rejects them, whether they arrive as a component binary
or as a precompiled one.

[containerd]: https://containerd.io/
[wasmtime]: https://wasmtime.dev/
[runwasi]: https://github.com/containerd/runwasi

#### Building the shim

```shell
cargo build --release -p containerd-shim-peridot
sudo cp target/release/containerd-shim-peridot-v1 /usr/local/bin/
```

containerd locates a shim by its binary name, so the file must keep the name
`containerd-shim-peridot-v1` and sit on containerd's `PATH`. That name is what makes the runtime
handler `io.containerd.peridot.v1` available below.

#### Getting Started

The containerd shim for Peridot utilizes OCI images following the [Wasm OCI artifact layout](https://tag-runtime.cncf.io/wgs/wasm/deliverables/wasm-oci-artifact/). These images can be created using the `oci-tar-builder` tool.

Firstly, ensure that `oci-tar-builder` is installed. You can install it using Cargo:

```shell
cargo install oci-tar-builder
```

Use `oci-tar-builder` to create an OCI image. The image must contain the `peridot-config.yaml`
configuration file, which is the ordinary Peridot configuration and is what selects the context
chain. Assuming the Wasm module is named `wasi-helloworld.wasm`:

```shell
oci-tar-builder --name wasi-helloworld \
                --repo localhost:5000 \
                --tag latest --module wasi-helloworld.wasm \
                --layer application/vnd.peridot.image.layer.v1+json=peridot-config.yaml \
                -o wasi-helloworld-oci.tar
```

The `--repo` above assumes your container registry is running at `localhost:5000`. You can either
use a remote repository or, for instance, set one up locally:

```shell
docker run -d -p 5000:5000 --name registry registry:2.7
```

Finally, import the OCI image into your container registry:

```shell
regctl image import localhost:5000/wasi-helloworld:latest wasi-helloworld-oci.tar
```

Run the image:

```shell
sudo ctr run --rm --net-host --runtime=io.containerd.peridot.v1 \
    localhost:5000/wasi-helloworld:latest wasi-helloworld /wasi-helloworld.wasm
```

The last argument is the path of the module inside the image, so it follows the name given to
`--module` above.
