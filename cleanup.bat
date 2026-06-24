@echo off
del final_clean.txt 2>nul
"C:\Program Files\Git\bin\git.exe" add -A
"C:\Program Files\Git\bin\git.exe" commit -m "chore: remove temp files (commit_out, do_commit, final_clean)"
echo done
