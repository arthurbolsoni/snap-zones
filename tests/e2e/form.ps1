param([switch]$Normal)
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.Form
$f.Text = "snaptest-janela"
$f.TopMost = $true
$f.StartPosition = 'Manual'
$f.Location = New-Object System.Drawing.Point(400, 200)
$f.Size = New-Object System.Drawing.Size(900, 600)
if (-not $Normal) { $f.Add_Shown({ $f.WindowState = 'Maximized' }) } else { $f.Add_Shown({ $f.WindowState = 'Maximized'; $f.WindowState = 'Normal' }) }
[System.Windows.Forms.Application]::Run($f)
