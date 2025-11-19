# Peridot: An I/O-Extensible Execution Environment for WebAssembly Containers
![Rust workflow](https://github.com/pablogs98/peridot/actions/workflows/rust.yml/badge.svg)

## Requirements

Install ```protoc```

```bash
sudo apt install protobuf-compiler
```

Install [IBM GEDS](https://github.com/IBM/GEDS):

```bash
install geds dependencies ----->
git clone https://github.com/IBM/GEDS
geds install commands (install dir must be /usr/local) set variables and add commands. ----->
```

## 📦 Build

Make sure you have Rust (1.73+) and cargo installed.

```bash
cargo build --release
```

To build with a specific feature/context, for example `token`:

```bash
cargo build --release --features token
```

---

## ▶️ Usage

```
peridot <CONFIG_PATH> --module <WASM_FILE> [OPTIONS]
```

### Arguments

| Argument          | Required | Description                           |
|-------------------|----------|---------------------------------------|
| `<CONFIG_PATH>`   | ✔️       | Path to the YAML config file          |
| `--module, -m`    | ✔️       | Path to the WebAssembly module to run |
| `--log-level, -l` | ✖️       | Log verbosity (default: `info`)       |
| `--overseer, -o`  | ✖️       | Path to overseer                      |

---

## 💡 Example Execution

```bash
peridot --features [s3, token, geds,...] config.yaml \
    --module path/to/module.wasm \
    --log-level debug
```

---

## 📝 Example `config.yaml`

```yaml
args:
  - "foo"
  - "bar"

io:
  demand: 150.0
  max_bandwidth: 300.0 

cpu:
  demand: 75.0
  utilization: 0.85
```

## Acknowledgements
<img src="https://user-images.githubusercontent.com/45240979/228180946-606cb75e-46c9-429c-a62b-ea9098c375a0.svg"  height="65">
This project has received funding from the European Union's Horizon Europe (HE) Research and Innovation Programme (RIA) under ...
