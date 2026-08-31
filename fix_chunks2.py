import sys

def replace_in_file(filepath, replacements):
    with open(filepath, 'r') as f:
        content = f.read()

    for old, new in replacements:
        content = content.replace(old, new)

    with open(filepath, 'w') as f:
        f.write(content)

replace_in_file('in-cli/src/inrt.rs', [
    ('b.try_into().unwrap()', '*b')
])

replace_in_file('in-cli/src/native_emit/lower/lower_tests.rs', [
    ('b.try_into().unwrap()', '*b'),
    ('chunk.try_into().expect("chunk")', '*chunk'),
    ('b.try_into().ok()', 'Some(*b)')
])
