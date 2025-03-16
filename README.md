A client for [WiFi File Transfer](https://play.google.com/store/apps/details?id=com.techprd.filetransfer) HTTP protocol. Lets you download all files from the specified remote directory (default "/DCIM/Camera") to local directory (default is "out/").

```powershell
$env:RUST_LOG="fast_sync=trace"; cargo r -q -- --ip 192.168.1.111 -d /Pictures
```
