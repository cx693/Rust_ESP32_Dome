[English](README.md) | [中文](README_CN.md)

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1780842853453-2f7c908d-8ee3-443d-b967-ae06d26315c0.png" width="767" title="" crop="0,0,1,1" id="YMKvO" class="ne-image">

# 基础工具安装
## 安装espflash
使用以下指令进行安装

```bash
cargo install espflash --locked
```

### 查看芯片版本
```bash
espflash board-info
```

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779637265046-485913f0-73bb-4a41-9b31-3199612b34c5.png" width="585" title="" crop="0,0,1,1" id="ue6b9ad5f" class="ne-image">



### 使用probe-rs进行调试接口验证（JTAG）
```bash
probe-rs info
```

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779637483576-364cb633-da77-43c9-9e2c-a104a1926f29.png" width="585" title="" crop="0,0,1,1" id="u38932606" class="ne-image">



## 安装Xtensa架构解释器
```bash
cargo install espup --locked
espup install
```

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779640231824-295eef39-fbf6-46f0-84b0-4d951166818d.png" width="585" title="" crop="0,0,1,1" id="uadb2b6fe" class="ne-image">

```bash
echo ". /Users/你的用户名/export-esp.sh" >> ~/.zshrc
source ~/.zshrc
```



## 安装模版工具
```bash
cargo install esp-generate --locked
```




# 工具创建工程模版
使用esp-generate工具创建工程模版

```bash
esp-generate
```

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779638273404-da5c3781-416c-41b2-863a-4cebbfc15e01.png" width="585" title="" crop="0,0,1,1" id="u786d7580" class="ne-image">

输入工程名字后进入配置界面

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779638323211-7a07f645-a1c0-43fa-8b79-c63c76982748.png" width="585" title="" crop="0,0,1,1" id="u95914f14" class="ne-image">

选择ESP类型

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779639121093-ddd81b3b-65a9-49dd-a440-ad44110d1afa.png" width="585" title="" crop="0,0,1,1" id="ufbf5c75f" class="ne-image">

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779638342590-2459523d-d176-4739-9753-a4e5ef8e001e.png" width="585" title="" crop="0,0,1,1" id="u0c18af49" class="ne-image">

设置选择下载器

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779688580767-cc8c0493-edf7-4eca-a9c0-8ace03e5d228.png" width="585" title="" crop="0,0,1,1" id="ubde2bc41" class="ne-image">

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779638794749-3bf242d4-59aa-4fff-8e7a-0b683d7435cd.png" width="585" title="" crop="0,0,1,1" id="ue6ed664e" class="ne-image">

设置使用编辑器

<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779638862759-fec91f67-21aa-49cb-b9e8-978f9555c7b5.png" width="585" title="" crop="0,0,1,1" id="u794e7379" class="ne-image">



# 编译
```bash
cargo build
```



<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779686523349-d2a07ad9-d027-4cd1-b57b-83dd7ea357c6.png" width="675" title="" crop="0,0,1,1" id="u392c4b9e" class="ne-image">



# 下载
```bash
cargo run
```



<img src="https://cdn.nlark.com/yuque/0/2026/png/67055297/1779686719283-792d08bc-961e-43c1-9ccf-5acaed932fe1.png" width="685" title="" crop="0,0,1,1" id="u85310682" class="ne-image">




## 解决索引问题导致无语法提示
已解决！替换文件 `.vscode/settings.json` 为以下内容：

```rust
{
    "rust-analyzer.cargo.allTargets": false,
    "rust-analyzer.cargo.target": null,
    "rust-analyzer.check.onSave.command": "clippy",
    "rust-analyzer.cargo.features": "default",
    "rust-analyzer.server.extraEnv": {
        "RUSTUP_TOOLCHAIN": "stable"
    }
}
```



# 调试
默认配置已OK，直接按F5即可！注意：目前在loop函数中设置断点效果不佳。
