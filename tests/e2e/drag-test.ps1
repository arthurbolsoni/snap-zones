# GrabY conta a partir do topo da área de trabalho (abaixo da barra, se houver).
param([int]$GrabX = 1500, [int]$GrabY = 12)
. "$PSScriptRoot\drag-lib.ps1"

$p, $h = OpenForm
if (-not $h -or $h -eq 0) { "janela de teste não encontrada; abortando sem mexer no mouse"; Stop-Process -Id $p.Id -ErrorAction SilentlyContinue; exit 1 }
Start-Sleep -Milliseconds 600
State $h "maximizada"

[void][T]::SetCursorPos($GrabX, (Monitor).work.T + $GrabY)
Start-Sleep -Milliseconds 200
[T]::Btn(2)  # LEFTDOWN
Start-Sleep -Milliseconds 150
for ($k = 0; $k -lt 6; $k++) { [T]::Move(14, 18); Start-Sleep -Milliseconds 8 }
Start-Sleep -Milliseconds 400
State $h "apos 1o arraste"
for ($k = 0; $k -lt 30; $k++) { [T]::Move(-6, 8); Start-Sleep -Milliseconds 15 }
Start-Sleep -Milliseconds 300
State $h "apos 2o arraste"
[T]::Btn(4)  # LEFTUP
Start-Sleep -Milliseconds 400
State $h "soltou"
for ($k = 0; $k -lt 10; $k++) { [T]::Move(20, 0); Start-Sleep -Milliseconds 10 }
Start-Sleep -Milliseconds 300
State $h "mexeu depois"
Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
