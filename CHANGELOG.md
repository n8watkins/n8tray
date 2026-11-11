# Changelog

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

