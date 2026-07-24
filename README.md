# AnySync Receiver 使用说明 / User Guide


AnySync Receiver 是一个电脑端接收工具。电脑启动后，手机可以通过浏览器把照片、视频和文档原样传到电脑指定文件夹，不压缩、不转码。

### 根目录文件

- `AnySyncReceiver.exe`：主程序，双击运行。
- `WebView2Loader.dll`：运行依赖，请和 exe 放在同一目录，不要删除。
- `source/anysync-receiver`：完整源码目录。

### 使用方法

1. 双击运行 `AnySyncReceiver.exe`。
2. 点击 `Change` 选择保存目录。
3. 点击 `Start Receiver` 启动接收服务。
4. 手机和电脑连接到同一个 Wi-Fi，或者手机开热点让电脑连接。
5. 用手机扫描电脑界面的二维码，或手动打开界面显示的 `Phone URL`。
6. 在手机浏览器选择照片、视频或文档，选择后会自动上传。
7. 上传完成后，文件会原样保存到电脑选择的目录。

### 源码开发

源码在：

```text
source/anysync-receiver
```

开发运行：

```powershell
cd source\anysync-receiver
npm install
npm run tauri:dev
```

### 注意事项

- Windows 防火墙弹窗时，请允许专用网络访问。
- 如果手机打不开地址，请确认手机和电脑在同一局域网。
- 如果使用手机热点，电脑需要连接到这个热点后再启动接收。
- 程序依赖 Microsoft Edge WebView2 Runtime。Windows 11 通常已自带。



AnySync Receiver is a desktop receiving tool. After starting it on your PC, your phone can upload original photos, videos, and documents through a browser to a selected PC folder. Files are saved as-is, with no compression or transcoding.

### Root Files

- `AnySyncReceiver.exe`: main application.
- `WebView2Loader.dll`: runtime dependency. Keep it in the same folder as the exe.
- `source/anysync-receiver`: full source code.

### How To Use

1. Run `AnySyncReceiver.exe`.
2. Click `Change` and choose a save folder.
3. Click `Start Receiver`.
4. Connect your phone and PC to the same Wi-Fi, or enable phone hotspot and connect the PC to it.
5. Scan the QR code on the PC screen, or open the shown `Phone URL` manually.
6. Select photos, videos, or documents in the phone browser. Upload starts automatically after selection.
7. Uploaded files are saved unchanged to the selected folder.

### Development

Source code is in:

```text
source/anysync-receiver
```

Run from source:

```powershell
cd source\anysync-receiver
npm install
npm run tauri:dev
```

### Notes

- Allow private network access if Windows Firewall asks.
- If the phone cannot open the URL, make sure both devices are on the same local network.
- When using a phone hotspot, connect the PC to the hotspot before starting the receiver.
- The app requires Microsoft Edge WebView2 Runtime. It is usually already installed on Windows 11.
