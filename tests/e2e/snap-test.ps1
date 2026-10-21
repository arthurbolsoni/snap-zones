# Encaixa a janela de teste na borda esquerda e depois a puxa rápido: deve voltar a 900x600 e seguir.
. "$PSScriptRoot\drag-lib.ps1"

$p = Start-Process pwsh -ArgumentList "-NoProfile","-STA","-File","$PSScriptRoot/form.ps1","-Normal" -WindowStyle Minimized -PassThru
$h = 0
for ($k = 0; $k -lt 30 -and $h -eq 0; $k++) {
  Start-Sleep -Milliseconds 300
  $h = [T]::FindWindowW([NullString]::Value, "snaptest-janela")
}

if (-not $h -or $h -eq 0) { "janela de teste não encontrada; abortando sem mexer no mouse"; Stop-Process -Id $p.Id -ErrorAction SilentlyContinue; exit 1 }
Start-Sleep -Milliseconds 600
State $h "inicial"

# barra de título da janela normal (400,200)-(1300,800): ponto em (850, 215)
[void][T]::SetCursorPos(850, 215)
Start-Sleep -Milliseconds 200
[T]::Btn(2)
for ($k = 0; $k -lt 40; $k++) { [T]::Move(-40, 0); Start-Sleep -Milliseconds 10 }  # até a borda esquerda
Start-Sleep -Milliseconds 400
[T]::Btn(4)
Start-Sleep -Milliseconds 500
State $h "encaixada"

# puxa de volta, rápido, pela barra de título
[void][T]::SetCursorPos(640, 15)
Start-Sleep -Milliseconds 200
[T]::Btn(2)
for ($k = 0; $k -lt 6; $k++) { [T]::Move(30, 20); Start-Sleep -Milliseconds 8 }
Start-Sleep -Milliseconds 400
State $h "puxou"
for ($k = 0; $k -lt 10; $k++) { [T]::Move(20, 10); Start-Sleep -Milliseconds 10 }
Start-Sleep -Milliseconds 300
State $h "seguiu"
[T]::Btn(4)
Start-Sleep -Milliseconds 300
for ($k = 0; $k -lt 10; $k++) { [T]::Move(20, 0); Start-Sleep -Milliseconds 10 }
Start-Sleep -Milliseconds 300
State $h "soltou+mexeu"
Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
