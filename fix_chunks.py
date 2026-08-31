import sys

def replace_in_file(filepath, replacements):
    with open(filepath, 'r') as f:
        content = f.read()

    for old, new in replacements:
        content = content.replace(old, new)

    with open(filepath, 'w') as f:
        f.write(content)

replace_in_file('in-cli/src/inrt.rs', [
    ('.chunks_exact(4)', '.as_chunks::<4>().0.iter()')
])

replace_in_file('in-cli/src/native_emit/lower/lower_tests.rs', [
    ('.chunks_exact(4)', '.as_chunks::<4>().0.iter()'),
    ('.chunks_exact(8)', '.as_chunks::<8>().0.iter()')
])
