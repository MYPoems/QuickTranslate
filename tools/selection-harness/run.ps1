<#
.SYNOPSIS
  Runs the selection capture against Notepad, Word or Excel using synthetic text.

.DESCRIPTION
  Opens a blank document created for the test, selects text in it, lets the harness play
  the hotkey and call the application's capture code, then closes the document without
  saving. Refuses to touch a Word or Excel session that is already open with anything else.

  The machine must be left alone while it runs: it sends real key presses to whichever
  window is in front and replaces the clipboard content.

.EXAMPLE
  ./run.ps1 -Stage notepad
  ./run.ps1 -Stage word -Release alt-first -Holds 40,150
#>
param(
    [Parameter(Mandatory = $true)][ValidateSet('notepad', 'word', 'excel')][string]$Stage,
    [int]$Repeat = 2,
    [int[]]$Holds = @(40, 150),
    [ValidateSet('key-first', 'alt-first')][string]$Release = 'key-first',
    [ValidateSet('text', 'empty', 'keep')][string]$Orig = 'text',
    [int]$Presses = 1,
    [string]$Hotkey = 'j',
    [switch]$NoSelection,
    [switch]$Build
)
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $here 'target' }
$exe = Join-Path $targetDir 'release\selection-harness.exe'
if ($Build -or -not (Test-Path -LiteralPath $exe)) {
    cargo build --release --manifest-path (Join-Path $here 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'harness build failed' }
}
$marker = 'QTDIAG'
$sample = 'QTDIAG sample: The quick brown fox jumps over the lazy dog.'
$rows = New-Object System.Collections.Generic.List[object]

function Invoke-Case([string]$Target, [string]$Label, [string]$PreKeys = '', [string]$PostKeys = '') {
    foreach ($hold in $Holds) {
        for ($i = 1; $i -le $Repeat; $i++) {
            $arguments = @('--target', $Target, '--label', $Label, '--hold-ms', $hold, '--marker', $marker,
                '--release', $Release, '--orig', $Orig, '--presses', $Presses, '--hotkey', $Hotkey)
            if ($PreKeys) { $arguments += @('--pre-keys', $PreKeys) }
            if ($PostKeys) { $arguments += @('--post-keys', $PostKeys) }
            $line = & $exe @arguments
            $rows.Add(($line | ConvertFrom-Json))
            Start-Sleep -Milliseconds 700
        }
    }
}

# A just-closed Office instance can take a few seconds to exit; do not mistake it for a user session.
function Wait-Exit([string]$Name) {
    for ($n = 0; $n -lt 40 -and (Get-Process $Name -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -eq 0 }); $n++) { Start-Sleep -Milliseconds 500 }
}

switch ($Stage) {
    'notepad' {
        $file = Join-Path $env:TEMP 'quicktranslate-selection-sample.txt'
        Set-Content -LiteralPath $file -Value $sample -Encoding utf8
        $before = @(Get-Process Notepad -ErrorAction SilentlyContinue | ForEach-Object Id)
        Start-Process notepad.exe -ArgumentList "`"$file`""
        Start-Sleep -Seconds 3
        try {
            if ($NoSelection) { Invoke-Case 'Notepad.exe' 'notepad-nosel' 'esc,ctrl+end' }
            else { Invoke-Case 'Notepad.exe' 'notepad' 'esc,ctrl+a' }
        } finally {
            Get-Process Notepad -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Id } | Stop-Process -Force
            Remove-Item -LiteralPath $file -ErrorAction SilentlyContinue
        }
    }
    'word' {
        Wait-Exit 'WINWORD'
        if (Get-Process WINWORD -ErrorAction SilentlyContinue) { throw 'Word is already running; close it first. The harness only works on a document it created.' }
        $word = New-Object -ComObject Word.Application
        try {
            $word.Visible = $true
            $document = $word.Documents.Add()
            $selection = $word.Selection
            $selection.TypeText($sample)
            $selection.TypeParagraph()
            $selection.TypeText('Second paragraph of synthetic text for selection diagnostics.')
            $selection.TypeParagraph()
            $table = $document.Tables.Add($selection.Range, 2, 2)
            $table.Cell(1, 1).Range.Text = 'QTDIAG cell one'
            $table.Cell(1, 2).Range.Text = 'cell two'
            Start-Sleep -Seconds 2

            # Esc first: each run starts with keyboard focus in the document.
            $document.Paragraphs.First.Range.Select()
            Invoke-Case 'WINWORD.EXE' 'word-body' 'esc'
            $table.Cell(1, 1).Range.Select()
            Invoke-Case 'WINWORD.EXE' 'word-cell' 'esc'
        } finally {
            try { $document.Close([ref]0) } catch {}
            try { $word.Quit([ref]0) } catch {}
            foreach ($reference in $table, $selection, $document, $word) { if ($reference) { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($reference) } }
            $table = $selection = $document = $word = $null
            [GC]::Collect(); [GC]::WaitForPendingFinalizers(); [GC]::Collect()
        }
    }
    'excel' {
        Wait-Exit 'EXCEL'
        if (Get-Process EXCEL -ErrorAction SilentlyContinue) { throw 'Excel is already running; close it first. The harness only works on a workbook it created.' }
        $excel = New-Object -ComObject Excel.Application
        try {
            $excel.Visible = $true
            $book = $excel.Workbooks.Add()
            $sheet = $book.ActiveSheet
            $sheet.Range('A1').Value2 = $sample
            $sheet.Range('B1').Value2 = 'QTDIAG second cell'
            $sheet.Range('A2').Value2 = 'third cell'
            $sheet.Range('B2').Value2 = 'fourth cell'
            Start-Sleep -Seconds 2

            [void]$sheet.Range('A1').Select()
            Invoke-Case 'EXCEL.EXE' 'excel-single' 'esc'
            [void]$sheet.Range('A1:B2').Select()
            Invoke-Case 'EXCEL.EXE' 'excel-multi' 'esc'
            [void]$sheet.Range('A1').Select()
            Invoke-Case 'EXCEL.EXE' 'excel-edit' 'esc,esc,f2,shift+home' 'esc,esc'
        } finally {
            try { $excel.CutCopyMode = 0 } catch {}
            try { $book.Close($false) } catch {}
            try { $excel.Quit() } catch {}
            # Excel only exits once every COM reference is gone, including the ones PowerShell
            # created for the sheet and the workbook.
            foreach ($reference in $sheet, $book, $excel) { if ($reference) { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($reference) } }
            $sheet = $book = $excel = $null
            [GC]::Collect(); [GC]::WaitForPendingFinalizers(); [GC]::Collect()
        }
    }
}

foreach ($row in $rows) {
    if ($row.abort) { '{0,-14} hold={1,-4} ABORT {2}' -f $row.label, $row.hold_ms, $row.abort; continue }
    $results = ($row.results | ForEach-Object { if ($_.result -eq 'ok') { "ok/$($_.ms)ms/marker=$($_.marker)" } else { "$($_.result)/$($_.ms)ms" } }) -join ' + '
    '{0,-14} {1,-9} hold={2,-4} fired={3}/{4} clipboard={5,-10} focus {6} -> {7}  {8}' -f `
        $row.label, $row.release, $row.hold_ms, $row.fired, $row.presses, $row.clipboard_after, $row.focus_before, $row.focus_after, $results
}
$expected = if ($NoSelection) { 'NO_SELECTED_TEXT' } else { 'ok' }
$bad = @($rows | Where-Object { $_.abort -or @($_.results | Where-Object { $_.result -ne $expected }).Count -gt 0 -or $_.focus_before -ne $_.focus_after })
if ($bad.Count) { "$($bad.Count) of $($rows.Count) runs did not behave as expected"; exit 1 }
"$($rows.Count) runs behaved as expected"
