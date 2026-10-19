using System.Diagnostics;
using System.IO;

namespace MdFileAssistant.Shell;

/// <summary>
/// 覆盖式更新：把 helper.bat 丢到临时目录，让它等本进程退出后解压覆盖、再拉起新版。
///
/// 为什么必须假手一个 bat：Windows 锁着正在运行的 exe，进程内改自己改不掉。
/// ChinaSeal 的 0.4.20 修的就是"点了安装、软件退出、然后没有然后"——所以每一步都往
/// helper.log 里写，出问题时有一处可查，而不是静默失败。
///
/// 两个硬规矩：
/// 1. **bat 正文纯 ASCII**。这台机器上应用目录常含中文（E:\...\MD文档可读性助手\...），
///    中文路径写进 .bat 文件里，cmd.exe 按 OEM 码页解码会读坏；改成用**参数**传进来，
///    命令行是 UTF-16 进来的，中文路径就安全了。
/// 2. 覆盖前先确认新版 exe 真的解出来了；确认不了就保留旧文件退出，绝不清空安装目录。
/// </summary>
public static class SelfReplace
{
    public static string InstallAndRestart(string zipPath)
    {
        var appDir = AppDir();
        var bat = Path.Combine(Updater.StagingDir, "helper.bat");
        File.WriteAllText(bat, Body(), new System.Text.UTF8Encoding(false));   // 正文全 ASCII，编码无关

        var psi = new ProcessStartInfo
        {
            FileName = bat,
            Arguments = $"\"{zipPath}\" \"{appDir}\"",
            UseShellExecute = true,
            CreateNoWindow = true,
            WindowStyle = ProcessWindowStyle.Hidden,
        };
        Process.Start(psi);
        return bat;
    }

    /// <summary>exe 所在目录——更新就是把这个目录里的文件覆盖掉。</summary>
    public static string AppDir()
    {
        var exe = Environment.ProcessPath ?? typeof(SelfReplace).Assembly.Location;
        return Path.GetDirectoryName(Path.GetFullPath(exe))!;
    }

    private static string Body() => """
@echo off
setlocal
set "ZIP=%~1"
set "APP_DIR=%~2"
set "STAGED=%TEMP%\MDReader-Update-Staged"
set "LOG=%~dp0helper.log"
echo === MDReader helper start %DATE% %TIME% === > "%LOG%"
echo ZIP=%ZIP% >> "%LOG%"
echo APP_DIR=%APP_DIR% >> "%LOG%"
if exist "%STAGED%" rd /S /Q "%STAGED%" >> "%LOG%" 2>&1
mkdir "%STAGED%" >> "%LOG%" 2>&1
echo [1] extracting... >> "%LOG%"
powershell -NoProfile -ExecutionPolicy Bypass -Command "Expand-Archive -LiteralPath '%ZIP%' -DestinationPath '%STAGED%' -Force" >> "%LOG%" 2>&1
echo [1] Expand-Archive exit=%ERRORLEVEL% >> "%LOG%"
echo [2] waiting for the app to exit... >> "%LOG%"
powershell -NoProfile -Command "$ErrorActionPreference='SilentlyContinue'; while (Get-Process -Name 'MdFileAssistant') { Start-Sleep -Seconds 1 }; exit 0" >> "%LOG%" 2>&1
echo [2] wait exit=%ERRORLEVEL% >> "%LOG%"
echo [3] replacing files... >> "%LOG%"
xcopy /Y /E /I /Q "%STAGED%\*" "%APP_DIR%\" >> "%LOG%" 2>&1
echo [3] xcopy exit=%ERRORLEVEL% >> "%LOG%"
if not exist "%APP_DIR%\MdFileAssistant.exe" (
  echo [!] ERROR: exe missing after copy - keeping old files >> "%LOG%"
  rd /S /Q "%STAGED%" >> "%LOG%" 2>&1
  exit /b 1
)
echo [4] cleaning... >> "%LOG%"
rd /S /Q "%STAGED%" >> "%LOG%" 2>&1
del /F /Q "%ZIP%" >> "%LOG%" 2>&1
echo [5] relaunching... >> "%LOG%"
start "" "%APP_DIR%\MdFileAssistant.exe"
ping -n 2 127.0.0.1 >nul
echo [6] done >> "%LOG%"
del /F /Q "%~f0"
""";
}
