# PostUI 文件审查服务

适用于 Linux amd64（x86_64）。兆芯开先处理器可直接使用。`postui` 会使用包内运行库启动，不依赖统信 UOS 20 自带的 glibc 版本。

## 启动

在本目录打开终端并执行：

~~~bash
./postui
~~~

请保留 `postui.bin`、`runtime`、`config.yaml` 和 `.postui/requests.yaml`。启动时显式指定全局配置：

~~~bash
./postui --config ./config.yaml
~~~

请求文件默认位于 `.postui/`。也可以用 `--requests /路径/到/请求集合.yaml` 临时切换接口集合。全局配置会设置主题和 JSON/变量高亮，请求集合只保存接口、变量和上传目录。

## 上传文档

将待审查文件放入本目录的 `files` 文件夹。在「上传普通审核文件」接口中填写变量 `review_file`，例如：

~~~text
项目可研报告.pdf
~~~

点击「发送」后，在响应区点击 `file_id` 的「提取」；随后填写一个未使用过的 `task_id`，再发送「提交文档审查」。

## 统信 UOS 剪贴板

变量的「填入」和响应字段的「提取」使用系统剪贴板。若桌面环境提示不可用，请通过系统软件源安装：Wayland 使用 `wl-clipboard`，X11 使用 `xclip` 或 `xsel`。

## 注意

- 接口地址固定为 `http://172.16.68.42/gmp/pms/info/hnlg/ai`，请确认笔记本可访问该地址。
- `app_key`、`app_secret`、`task_id`、`review_file` 默认为空，按实际联调信息填写。
- 「更新网络配置」会修改服务端网络地址，请确认变量内容后再发送。
