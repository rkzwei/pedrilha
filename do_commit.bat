@echo off
"C:\Program Files\Git\bin\git.exe" rm --cached commit_msg.txt commit_out.txt do_commit.bat gs.txt gc.txt ga.txt 2>nul
del commit_msg.txt commit_out.txt commit_out.txt gs.txt gc.txt ga.txt 2>nul
"C:\Program Files\Git\bin\git.exe" add -A
"C:\Program Files\Git\bin\git.exe" commit -m "chore: remove temp commit artifacts"
