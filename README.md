# unzipper

## Why

I often download zip files, and manually extracting them became tedious. Esp. double checking if I could delete the zip file or if I already had extracted it.

Hence: unzipper.

## What is it?

It a watches a folder for file changes, and if it finds archive folders, they will be extracted.

After extraction, it deletes the archive files.

### Where?

Defaults to the download folder as defined by the user. Can be overwritten with option in the command.

It only watches the folder itself, it doesn't watch them recursivly.

## Example

NOTE: I am not affiliated with getsamplefiles.com
Use them and their sample files at your own risk.

Assuming you are on Windows 11 and your browser stores downloaded files in:

`%USERPROFILE%\Downloads`

So if you download an archive file, e.g.:

https://getsamplefiles.com/download/zip/sample-1.zip

it will be saved at:

`%USERPROFILE%\Downloads\sample-1.zip`

Directly after, it will be extraced under:

`C:\Users\Philipp\Downloads\sample-1.zip (001)`

As in: The filename will become the directory name, and a numeric suffic `(xxx)` will be added. This is to ensure that even if you download a file of the same name again, it will get extracted into its own target directory.

### OS

Should work on all OS, yet was only tested on Windows 11 as of yet.

### Usage

```
.\unzipper.exe -h
Usage: unzipper.exe [OPTIONS]

Options:
  -p, --watch-path <WATCH_PATH>
  -h, --help                     Print help
  -V, --version                  Print version
```

```
PS C:\Users\Philipp\foo> .\unzipper.exe
[Main] Target directory set to: C:\Users\Philipp\Downloads
[Worker 0] Starting up.
[Main] Checking for existing archives in C:\Users\Philipp\Downloads...
[Worker 1] Starting up.
[Worker 2] Starting up.
[Worker 3] Starting up.
[Main] Watching directory: C:\Users\Philipp\Downloads for new archives...
[Main] Detected file event for: C:\Users\Philipp\Downloads\sample-1.zip. Sending to worker.
[Worker 0] Processing zip file: C:\Users\Philipp\Downloads\sample-1.zip
[Worker 0] Unzipping file: C:\Users\Philipp\Downloads\sample-1.zip to C:\Users\Philipp\Downloads\sample-1.zip (002)
[Worker 0] Extracting: sample-1/
[Worker 0] Extracting: sample-1/sample-1.webp
[Worker 0] Extracting: __MACOSX/sample-1/._sample-1.webp
[Worker 0] Extracting: sample-1/sample-1_1.webp
[Worker 0] Extracting: __MACOSX/sample-1/._sample-1_1.webp
[Worker 0] Extracting: sample-1/sample-5.webp
[Worker 0] Extracting: __MACOSX/sample-1/._sample-5.webp
[Worker 0] Extracting: sample-1/sample-5 (1).jpg
[Worker 0] Extracting: __MACOSX/sample-1/._sample-5 (1).jpg
[Worker 0] Successfully unzipped C:\Users\Philipp\Downloads\sample-1.zip
[Worker 0] Successfully deleted original archive: C:\Users\Philipp\Downloads\sample-1.zip
...
```

It will wait for other file events in the background.

### Install

Currently need to compile from rust src.

### TODO

- provide background service with autostart
- maybe delay deletion of file

# Known issues

- browsers (e.g. Chrome) may complain after download that they could not check for viruses. This is due to the fact that the file is deleted right once it is fully extracted.

### License

Dual MIT / GLP, will explicate later.
