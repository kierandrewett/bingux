import pytest

from bxdesktop import keysyms

pytestmark = pytest.mark.fast


def test_named_keys_and_single_characters():
    assert keysyms.keysym("Return") == 0xFF0D
    assert keysyms.keysym("F2") == 0xFFBF
    assert keysyms.keysym("a") == ord("a")
    assert keysyms.keysym("Super_L") == 0xFFEB


def test_characters_outside_latin_1_use_the_unicode_range():
    assert keysyms.keysym_for_character("é") == 0xE9
    assert keysyms.keysym_for_character("€") == 0x1000000 + 0x20AC
    assert keysyms.keysym_for_character("\n") == 0xFF0D


def test_an_unknown_key_name_is_an_error():
    with pytest.raises(KeyError):
        keysyms.keysym("NoSuchKey")
