//! 한글 자소 결합(NFD → NFC), 요구사항 10.6.
//!
//! macOS 는 파일 이름과 일부 편집기 출력에서 한글을 첫소리·가운뎃소리·끝소리로
//! 나눠 저장한다(이른바 자소 분리). 그대로 두면 `ㅎㅏㄴ` 처럼 보이고 표시 폭
//! 계산도 어긋나므로, 렌더링 입력을 읽어 들일 때 미리 하나의 음절로 합친다.
//!
//! 유니코드 표준의 한글 조합 알고리즘을 그대로 쓴다. 한글 외의 결합 문자는
//! 건드리지 않는다 — 외부 `unicode-normalization` 크레이트 없이 이 알고리즘만으로
//! 충분하다(mdview, MIT, `src/hangul.rs`를 그대로 채택 — THIRD_PARTY.md 참조).

const S_BASE: u32 = 0xAC00;
const L_BASE: u32 = 0x1100;
const V_BASE: u32 = 0x1161;
const T_BASE: u32 = 0x11A7;
const L_COUNT: u32 = 19;
const V_COUNT: u32 = 21;
const T_COUNT: u32 = 28;
const N_COUNT: u32 = V_COUNT * T_COUNT; // 588
const S_COUNT: u32 = L_COUNT * N_COUNT; // 11172

fn is_leading(c: u32) -> bool {
    (L_BASE..L_BASE + L_COUNT).contains(&c)
}

fn is_vowel(c: u32) -> bool {
    (V_BASE..V_BASE + V_COUNT).contains(&c)
}

/// 끝소리. `T_BASE`(채움 문자) 자체는 제외한다.
fn is_trailing(c: u32) -> bool {
    (T_BASE + 1..T_BASE + T_COUNT).contains(&c)
}

/// 끝소리를 붙일 수 있는 음절(받침 없는 완성형)인지.
fn is_lv_syllable(c: u32) -> bool {
    (S_BASE..S_BASE + S_COUNT).contains(&c) && (c - S_BASE) % T_COUNT == 0
}

/// 나뉘어 있는 한글 자소를 음절로 합친다. 합칠 것이 없으면 원본을 그대로 돌려준다.
pub fn compose(s: &str) -> String {
    if !needs_compose(s) {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i] as u32;
        // 첫소리 + 가운뎃소리 [+ 끝소리]
        if is_leading(c) {
            if let Some(&next) = chars.get(i + 1) {
                if is_vowel(next as u32) {
                    let l = c - L_BASE;
                    let v = next as u32 - V_BASE;
                    let mut syllable = S_BASE + (l * V_COUNT + v) * T_COUNT;
                    i += 2;
                    if let Some(&t) = chars.get(i) {
                        if is_trailing(t as u32) {
                            syllable += t as u32 - T_BASE;
                            i += 1;
                        }
                    }
                    out.push(char::from_u32(syllable).unwrap_or(chars[i - 1]));
                    continue;
                }
            }
        }
        // 이미 합쳐진 음절 + 끝소리
        if is_lv_syllable(c) {
            if let Some(&t) = chars.get(i + 1) {
                if is_trailing(t as u32) {
                    out.push(char::from_u32(c + (t as u32 - T_BASE)).unwrap_or(chars[i]));
                    i += 2;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// 조합할 자소가 하나라도 있는지 빠르게 살핀다.
fn needs_compose(s: &str) -> bool {
    s.chars().any(|c| {
        let u = c as u32;
        (L_BASE..=0x11FF).contains(&u) || (0xA960..=0xA97C).contains(&u) || (0xD7B0..=0xD7FB).contains(&u)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_decomposed_hangul() {
        // "한글" 을 자소로 나눈 형태
        let nfd = "\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}";
        assert_eq!(compose(nfd), "한글");
    }

    #[test]
    fn composes_syllable_without_final() {
        assert_eq!(compose("\u{1102}\u{1161}"), "나");
    }

    #[test]
    fn attaches_final_to_precomposed_syllable() {
        // 가운뎃소리까지만 합쳐진 "가" + 끝소리 ㅁ
        assert_eq!(compose("가\u{11b7}"), "감");
    }

    #[test]
    fn leaves_normal_text_untouched() {
        assert_eq!(compose("이미 정상입니다 abc 123"), "이미 정상입니다 abc 123");
        assert_eq!(compose(""), "");
    }

    #[test]
    fn keeps_standalone_compatibility_jamo() {
        // ㄱ, ㅏ 는 낱자로 쓰이는 호환 자모이므로 합치지 않는다.
        assert_eq!(compose("ㄱㅏ"), "ㄱㅏ");
    }

    #[test]
    fn keeps_lone_jamo_that_cannot_combine() {
        assert_eq!(compose("\u{1100}"), "\u{1100}");
        assert_eq!(compose("\u{11ab}가"), "\u{11ab}가");
    }

    #[test]
    fn composed_text_has_correct_display_width() {
        use unicode_width::UnicodeWidthStr;
        let nfd = "\u{1112}\u{1161}\u{11ab}";
        assert_eq!(UnicodeWidthStr::width(compose(nfd).as_str()), 2);
    }

    #[test]
    fn mixed_document_composes_only_hangul() {
        let nfd = "# \u{1112}\u{1161}\u{11ab} title\n\n- caf\u{e9}\n";
        assert_eq!(compose(nfd), "# 한 title\n\n- café\n");
    }
}
