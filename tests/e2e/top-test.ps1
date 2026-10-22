# Borda de cima com uma AppBar no topo (barra): durante o arraste o Windows segura o cursor no
# pé dela, então a faixa da AppBar vale como topo.
# 1) arrastar pela barra de título até o meio da AppBar: maximiza.
# 2) redimensionar pela borda de cima até o meio da AppBar: altura toda, mesma largura.
# 3) Alt + arrastar até o alto da tela, no centro (notch): maximiza.
# Sem AppBar no topo os casos 1 e 2 param a 1 px do topo do monitor.
. "$PSScriptRoot\drag-lib.ps1"

$mi = Monitor
$barY = [Math]::Max($mi.mon.T + 1, [int](($mi.mon.T + $mi.work.T) / 2))
"monitor topo={0} area de trabalho topo={1} cursor vai a y={2}" -f $mi.mon.T, $mi.work.T, $barY

# leva o cursor até (x, y) em passos pequenos; para onde o Windows segurar
function MoveTo($x, $y) {
  for ($k = 0; $k -lt 200; $k++) {
    $c = New-Object T+PT; [void][T]::GetCursorPos([ref]$c)
    $dx = [Math]::Max(-12, [Math]::Min(12, $x - $c.x))
    $dy = [Math]::Max(-12, [Math]::Min(12, $y - $c.y))
    if ($dx -eq 0 -and $dy -eq 0) { break }
    [T]::Move($dx, $dy); Start-Sleep -Milliseconds 10
  }
}
function Reset($h) {
  [void][T]::ShowWindow($h, 9)  # SW_RESTORE
  [void][T]::SetWindowPos($h, [IntPtr]::Zero, 400, 200, 900, 600, 4)  # SWP_NOZORDER
  Start-Sleep -Milliseconds 500
}

$p, $h = OpenForm -Normal
if (-not $h -or $h -eq 0) { "janela de teste não encontrada; abortando sem mexer no mouse"; Stop-Process -Id $p.Id -ErrorAction SilentlyContinue; exit 1 }
Start-Sleep -Milliseconds 600
Reset $h
State $h "inicial"

# 1) barra de título da janela normal (400,200)-(1300,800): ponto em (850, 215)
[void][T]::SetCursorPos(850, 215)
Start-Sleep -Milliseconds 200
[T]::Btn(2)
MoveTo 850 $barY
Start-Sleep -Milliseconds 400
[T]::Btn(4)
Start-Sleep -Milliseconds 500
State $h "1 titulo"

# 2) borda de cima até a AppBar
Reset $h
[void][T]::SetCursorPos(850, 201)
Start-Sleep -Milliseconds 200
[T]::Btn(2)
MoveTo 850 $barY
Start-Sleep -Milliseconds 400
[T]::Btn(4)
Start-Sleep -Milliseconds 500
State $h "2 borda"

# 3) Alt + arraste até o alto, no centro do monitor
Reset $h
$cx = [int](($mi.mon.L + $mi.mon.R) / 2)
[void][T]::SetCursorPos(850, 215)
Start-Sleep -Milliseconds 200
[T]::Btn(2)
MoveTo 850 400
[T]::keybd_event(0x12, 0, 0, [IntPtr]::Zero)  # Alt
MoveTo $cx $mi.mon.T
Start-Sleep -Milliseconds 400
[T]::Btn(4)
[T]::keybd_event(0x12, 0, 2, [IntPtr]::Zero)
Start-Sleep -Milliseconds 500
State $h "3 alt notch"
Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
