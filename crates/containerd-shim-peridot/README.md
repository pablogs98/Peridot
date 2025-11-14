## containerd-shim-peridot

This is a [containerd] shim for running WebAssembly modules and components using Peridot. This shim is heavily based on the [Wasmtime] containerd shim.

[containerd]: https://containerd.io/
[wasmtime]: https://wasmtime.dev/

#### Getting Started
The containerd shim for Peridot utilizes OCI images following the [Wasm OCI artifact layout](https://tag-runtime.cncf.io/wgs/wasm/deliverables/wasm-oci-artifact/). These images can be created using the `oci-tar-builder` tool.

Firstly, ensure that `oci-tar-builder` is installed. You can install it using Cargo:

```shell
cargo install oci-tar-builder
```

Use `oci-tar-builder` to create an OCI image. The image must contain the `peridot-config.yaml` configuration file.  Assuming the Wasm module is named `wasi-module.wasm`:

```shell
bin oci-tar-builder -- \
    --name wasi-helloworld \
    --repo localhost:5000 \
    --tag latest --module wasi-helloworld.wasm \
    --layer peridot-config.yaml \
    -o hello-oci.tar
```

The previous command assumes your container registry is running at `localhost:5000`. You can either use a remote repository or, for instance, set up one locally:

```shell
docker run -d -p 5000:5000 --name registry registry:2.7
```

Finally, import the OCI image into your container registry:

```shell
regctl image import localhost:5000/wasi-helloworld:latest hello-oci.tar 
```

Run the image

```shell
sudo ctr run --rm --net-host --runtime=io.containerd.peridot.v1 \
    localhost:5000/wasi-helloworld:latest wasi-helloworld /wasi-helloworld.wasm
```


