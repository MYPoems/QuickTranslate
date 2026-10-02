# 中英文离线音色插件

设置 → 朗读 → 离线朗读 → 一键安装中英文音色。下载约 163.9 MiB，可取消；大小与固定 SHA-256 全部通过后才启用。无需管理员权限、Python 或额外 API，文字不上传。卸载只删除本应用音色插件，Windows 系统音色不受影响。v1.2.1 默认选择云端流式朗读，需独立配置 Key；本插件仍可明确选择使用。

插件采用 Kokoro v1.1 中英文 INT8 模型和 sherpa-onnx 1.13.8 Windows x64 CPU 独立运行时，不重新分发 Windows 系统音色。提供三种中文和三种英文精选音色。自动选择优先使用已有 Windows 音色；缺少对应语言时使用已安装插件。显式选择插件音色后始终使用该音色，失败不会悄悄替换。

v1.2.1 将模型加载到独立工作进程，连续段落复用模型，闲置 5 分钟后退出。主 UI 进程不加载第三方语音 DLL。CPU 线程数可选择 2/4/6/8（默认 4）；不要假定线程越多越快。合成超时或取消会丢弃工作进程，下一次重新初始化。文本仅通过匿名管道传输，WAV 在内存编码，不再写临时音频文件；会话音频缓存最多 24 MiB/64 条。主安装包不携带约 164 MiB 下载资源。插件在用户数据目录 `speech-plugins/kokoro-v1.1/`，升级不会删除它。

## 来源与许可

- 模型：<https://huggingface.co/hexgrad/Kokoro-82M-v1.1-zh>，Apache-2.0。模型包保留原始 `LICENSE`。
- 推理运行时和完整源码：<https://github.com/k2-fsa/sherpa-onnx/tree/v1.13.8>，主项目 Apache-2.0：<https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/LICENSE>。
- 发音数据/第三方依赖包含 eSpeak NG（GPL-3.0）：<https://github.com/espeak-ng/espeak-ng>，完整源码及许可：<https://github.com/espeak-ng/espeak-ng/blob/master/COPYING>。v1.2.1 工作进程通过动态加载 sherpa C API 调用；进程隔离不豁免许可义务。公开分发前必须重新审计 GPL 依赖的链接、许可证及对应源码提供要求。本轮仅本地开发验证，不发布 Release。
- ONNX Runtime：<https://github.com/microsoft/onnxruntime>，MIT；许可证：<https://github.com/microsoft/onnxruntime/blob/main/LICENSE>。

下载仅从上游官方 GitHub 固定版本获取，下载后验证以下 SHA-256（对应 GitHub release asset digest，已本地验证）：

| 包 | 字节数 | SHA-256 |
| --- | ---: | --- |
| sherpa-onnx-v1.13.8-win-x64-shared-MT-Release.tar.bz2 | 24805859 | `6DFFDC715A4465B989446A6105265D2CB345E7101591A17D35534B6758F6E8DF` |
| kokoro-int8-multi-lang-v1_1.tar.bz2 | 147031220 | `A1E94694776049035C4F2C6529F003AAECE993C76AAE9A78995831C3C4DCAFC6` |

安全解压拒绝链接、越界路径和过大内容；安装及每次工作进程加载时验证可执行程序、DLL、模型、音色、词表及词典的内置固定哈希，包括新加载的 C API DLL 与其同目录依赖。已验证会话内只检查大小/修改时间，避免每段完整扫描；检测到变化后清理缓存并重验，完整性失败不运行。元数据缓存是性能优化，不能代替新进程加载前的哈希验证。安装失败不会启用半成品。
