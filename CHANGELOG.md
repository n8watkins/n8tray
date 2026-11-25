# Changelog

## 0.5.0 - 2026-11-25

### New

- New n8builds icons, with a tray icon that matches a light or dark taskbar ([#36](https://github.com/n8watkins/n8tray/pull/36))

## 0.4.1 - 2026-11-25

### Fixed

- Windows shows n8builds.dev as the publisher ([#34](https://github.com/n8watkins/n8tray/pull/34))

## 0.4.0 - 2026-11-24

PowerShell and Node apps start again on Windows, safer handling of apps the tray did not start, and low-memory alerts are off unless you turn them on.

### New

- Low-memory notifications are off unless you turn them on ([#30](https://github.com/n8watkins/n8tray/pull/30))

### Fixed

- An app N8 Tray did not start is never force-stopped ([#27](https://github.com/n8watkins/n8tray/pull/27))
- PowerShell and node apps start on Windows again ([#25](https://github.com/n8watkins/n8tray/pull/25))
- The memory line shows small amounts in MB, and a few quiet host fixes ([#29](https://github.com/n8watkins/n8tray/pull/29))
- An app found running is adopted only when the process is really the app's ([#26](https://github.com/n8watkins/n8tray/pull/26))
- The tray says so when its app monitor fails, and starts it again ([#28](https://github.com/n8watkins/n8tray/pull/28))
- On Windows, apps stay stoppable after the app monitor restarts ([#31](https://github.com/n8watkins/n8tray/pull/31))

## 0.3.0 - 2026-11-10

### New

- The hub lists running apps first and says once that the PowerShell tray manages them ([#16](https://github.com/n8watkins/n8tray/pull/16))

### Faster

- The closed window hands its memory back after 10 s instead of 60 s ([#17](https://github.com/n8watkins/n8tray/pull/17))

## 0.2.1 - 2026-11-10

First published release: ships everything listed under 0.2.0 (host core, tray, resource tracking, app icons, the hub and signed one-click updates), whose release failed its checks.

## 0.2.0 - 2026-11-10

### New

- Add the host core that discovers, supervises and verifies apps ([#5](https://github.com/n8watkins/n8tray/pull/5))
- Add the tray app with its icon, menu, window and sign-in start ([#6](https://github.com/n8watkins/n8tray/pull/6))
- Measure every app's memory, CPU and GPU, keep its history and alert on its budget ([#8](https://github.com/n8watkins/n8tray/pull/8))
- Apps can ship an SVG icon, with icons for the five example apps ([#9](https://github.com/n8watkins/n8tray/pull/9))
- The hub shows every app with its own icon, one action per row, panels and status details ([#10](https://github.com/n8watkins/n8tray/pull/10))
- Install N8 Tray updates in place with one click, signed and verified ([#11](https://github.com/n8watkins/n8tray/pull/11))

## 0.1.0 - 2026-11-09

### New

- N8 Tray's app contract is published: the manifest an app installs, the status and resource figures it reports, the shared status vocabulary, the security rules and the host lease, with JSON Schemas and example manifests.
- App authors can check their manifests and status documents, probe a running app's security, status and resources, and test their tray icon handling against a test tray, with the new n8tray-contract tool.

