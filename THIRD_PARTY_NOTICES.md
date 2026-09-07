# OziiDPI Third-Party Notices

This file contains third-party software notices and licenses.

---

## SpoofDPI

**Source:** https://github.com/xvzc/SpoofDPI  
**Version:** v1.2.1  
**License:** Apache License 2.0

SpoofDPI is used as the DPI bypass engine. OziiDPI builds it from source and bundles the resulting binary.

```
Copyright 2024 xvzc

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

---

## WinDivert

**Source:** https://github.com/basil00/WinDivert  
**Version:** 2.2 series  
**License:** GNU Lesser General Public License v3 or GNU General Public License v2

WinDivert provides the Windows packet interception driver and user-mode library.
The complete license text is distributed at `backend/deps/windivert/LICENSE`.

---

## Tauri

**Source:** https://github.com/tauri-apps/tauri  
**License:** Apache License 2.0 / MIT

---

## Lucide React

**Source:** https://github.com/lucide-icons/lucide  
**License:** ISC License

---

## httparse

**Source:** https://github.com/seanmonstar/httparse  
**License:** Apache License 2.0 / MIT

---

## thiserror

**Source:** https://github.com/dtolnay/thiserror  
**License:** Apache License 2.0 / MIT

---

## serde / serde_json

**Source:** https://github.com/serde-rs/serde  
**License:** Apache License 2.0 / MIT

---

## winreg

**Source:** https://github.com/nickelc/winreg  
**License:** MIT

---

## clap

**Source:** https://github.com/clap-rs/clap  
**License:** Apache License 2.0 / MIT

---

For the complete list of Rust dependencies, run:
```
cargo tree --manifest-path backend/Cargo.toml
```

For the complete list of frontend dependencies, run:
```
npm ls --all --prefix app
```
