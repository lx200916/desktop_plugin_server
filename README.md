# desktop_plugin_server
## Config
This application stores its conifg files at `C:\Users\{UserName}\AppData\Roaming\Cider Desktop Plugin\config\default-config.toml`.

Default Config:
```
# for Background Color,note that the format is RGBA,but each element is float( do not forget `dot`) and ranges in (0,1).if u set it to (1,255) we will divide it by 255. 
bg_color = [
    0.0,
    0.0,
    0.0,
    0.5,
]
# for Text Color. Same format as listed above. 
text_color = [
    1.0,
    1.0,
    1.0,
    1.0,
]
# Font Size in dp Unit.
font_size = 24
```
