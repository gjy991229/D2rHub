export const AUDIO_MOD_NAME_MAX_LENGTH = 128;

const WINDOWS_RESERVED_NAMES = /^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/i;

export function validateAudioModName(value: string): string | null {
  const name = value.trim();
  if (!name) return "请输入新 Mod 名称";
  if (name.length > AUDIO_MOD_NAME_MAX_LENGTH) {
    return `名称不能超过 ${AUDIO_MOD_NAME_MAX_LENGTH} 个字符`;
  }
  if (!/^[A-Za-z0-9_-]+$/.test(name)) {
    return "仅可使用英文字母、数字、短横线和下划线";
  }
  if (WINDOWS_RESERVED_NAMES.test(name)) {
    return "该名称是 Windows 保留名称，请换一个";
  }
  return null;
}
