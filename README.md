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

#Example

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

### License

Dual MIT / GLP, will explicate later.
