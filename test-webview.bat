@echo off
echo Testing WebView2 directly...
echo.

REM Create a minimal HTML file
echo ^<html^>^<body^>^<h1^>WebView2 Test^</h1^>^</body^>^</html^> > test.html

REM Use Edge to test WebView2
echo Opening test.html in Edge (which uses WebView2)...
start msedge.exe "%cd%\test.html"

echo.
echo If Edge opens with the test page, WebView2 is working.
echo.
pause