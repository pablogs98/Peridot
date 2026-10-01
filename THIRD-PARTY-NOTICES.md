# Third-party notices

The [LICENSE](LICENSE) at the repository root, Apache License 2.0, covers the
original work in this repository. It does not cover the material below, which
is redistributed here under its own terms. Nothing in this file grants rights
the upstream owners have not granted.

---

## Redistributed in this repository

### Sample images

`crates/contexts/imagenet-preprocessing/resources/banana.jpg`,
`cougar.jpg`, `tabby.png`, and the copies in `artifact/guests/resources/`.

Sample images used as input to the pre-processing workload. The copyright in
these images is held by their respective owners, not by the authors of this
repository. They are included solely so that the experiment has input data.

---

## Fetched at build time, not redistributed

The Rust dependencies in `Cargo.lock` are downloaded by Cargo when you build;
no copy of them is stored in this repository. They carry their own licenses,
predominantly MIT and Apache-2.0. Two are worth naming:

| Dependency | License | Note |
|---|---|---|
| `wasmtime`, `wasmtime-wasi`, `wiggle` | Apache-2.0 WITH LLVM-exception | The runtime Peridot is built on |
| `containerd-shim-wasm` (runwasi fork) | Apache-2.0 | Fetched from a pinned revision of <https://github.com/miqalvarez/runwasi>; used only by `containerd-shim-peridot` |

To produce a full dependency license inventory:

```bash
cargo install cargo-about
cargo about generate --output-file licenses.html
```

## Pulled at run time, not redistributed

The artifact's `docker-compose.yml` pulls container images that are not part
of this repository and remain under their own licenses:

| Image | License |
|---|---|
| `quay.io/minio/minio` | GNU AGPL v3 |
| `rust`, `debian` (build and runtime base images) | See the respective Docker Official Image repositories |

MinIO is used unmodified as a local S3-compatible endpoint for the storage
experiments. It is never linked into, combined with, or distributed alongside
Peridot's code, so its AGPL terms do not reach this repository.
