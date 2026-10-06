import pathlib, sys, tomllib
expression = pathlib.Path(sys.argv[1]).read_text().strip()
# And the faces the pages are drawn with, under page/fonts/.
expression = "(" + expression + ") AND OFL-1.1"
for file in pathlib.Path('packages.pekit').glob('dev.*.toml'):
    actual = tomllib.loads(file.read_text())['package']['license']
    assert actual == expression, f"{file}: expected {expression}, got {actual}"
