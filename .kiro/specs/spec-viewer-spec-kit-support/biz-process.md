# BizProcess — spec-viewer-spec-kit-support

## 정의
개발자를 위해 GitHub spec-kit(`.specify/`+`specs/<NNN-이름>/`) 프로젝트를 열어 기능별 진행 상태를 한눈에 확인하고 문서를 조회하는, `.kiro` 프로젝트 열람과 동등한 사용자 시각 프로세스이다.

## 가치사슬 매핑

| L1 Process | Unit Process (valueChainRef) | 가치 | 관련 요구사항 |
|------------|------------------------------|------|---------------|
| BP-SPEC-KIT-VIEW | VC-DEV-SPEC-VIEW | 터미널에서 스펙·문서를 읽기 전용으로 빠르게 파악한다(`.kiro`에 국한되지 않는 "스펙 열람" 가치를 spec-kit 프로젝트로 확장) | 1 ~ 9 |

### L2/L3 ↔ 요구사항 추적

| 노드 | 이름 | 관련 요구사항 |
|------|------|---------------|
| L2-B1 | 프로젝트 인식과 진입 | 1.1~1.4 |
| L2-B2 | 기능 트리 탐색 | 2.1~2.5 |
| L2-B3 | 진행 상태 확인 | 3.1~3.4, 4.1~4.5, 5.1~5.5 |
| L2-B4 | 체크박스 진행률 확인 | 6.1, 6.2 |
| L2-B5 | 문서 내용 확인 | 7.1, 7.2 |
| L2-B6 | 읽기 전용 신뢰 | 8.1 |
| L2-B7 | 편집모드 자연 연동 | 9.1 |
| L3-F1 | 실행 인자 + 루트 탐색(화면 비가시, 결과로 관찰) | 1.1~1.4 |
| L3-F2 | 기능 트리 패널 | 2.1~2.5 |
| L3-F3a | spec-kit 기능 배지 | 3.1~3.4, 4.1~4.5 |
| L3-F3b | `.kiro` 스펙 배지 | 3.1~3.4, 5.1~5.5 |
| L3-F4 | tasks.md 진행률 배지 | 6.1, 6.2 |
| L3-F5 | 문서 패널 | 7.1, 7.2 |
| L3-F6 | (화면 없음 — 결과로만 관찰) | 8.1 |
| L3-F7 | 편집 키 | 9.1 |

---

## L1 Process: spec-kit 프로젝트 열람  (id: BP-SPEC-KIT-VIEW, valueChainRef: VC-DEV-SPEC-VIEW)

개발자가 spec-kit으로 관리되는 프로젝트 디렉터리에서 뷰어를 열면, `.kiro` 프로젝트와 똑같은 방식으로 기능별 진행 상태와 문서를 조회한다.

```
L2-B1 Activity: 프로젝트 인식과 진입  (1.1~1.4)
  L3-F1 UI: 실행 인자 + 루트 탐색(화면 비가시 — 개발자는 트리가 뜨는 결과로만 확인)
    L4-S1 Step: 뷰어를 실행한다
      L5-D1 DetailStep: 더 가까운 위치에 `.specify/`만 있다
        Logic(AST):
          - IF `--all` 미지정 AND 탐색 경로 상 가장 가까운 마커가 `.specify/`(같은 디렉터리에 `.kiro/`도 있으면 `.kiro/` 우선) THEN spec-kit 모드로 열리고 기능 트리가 뜬다
      L5-D2 DetailStep: `.kiro/`와 `.specify/`가 같은 디렉터리에 함께 있다
        Logic(AST):
          - IF 두 마커가 한 디렉터리에 공존 THEN `.kiro/`가 선택되어 기존 동작 그대로 유지된다
      L5-D3 DetailStep: 위로 아무 마커도 없다
        Logic(AST):
          - IF `.kiro/`도 `.specify/`도 못 찾음 THEN 탐색 경로 포함 오류가 출력되고 비제로 종료(기존과 동일)
      L5-D4 DetailStep: `--all`이 함께 지정됐다
        Logic(AST):
          - IF `--all` THEN 마커 유무와 무관하게 일반 마크다운 트리로 연다(기존과 동일, 최우선)

L2-B2 Activity: 기능 트리 탐색  (2.1~2.5)
  L3-F2 UI: 기능 트리 패널
    L4-S2 Step: 기능 트리를 살펴본다
      L5-D5 DetailStep: `specs/` 아래 기능 디렉터리들이 있다
        Logic(AST):
          - `specs/<NNN-이름>/` 각각이 노드로 나열된다(디렉터리명 그대로)
      L5-D6 DetailStep: 기능 노드를 펼친다
        Logic(AST):
          - 실제 존재하는 문서만 `spec.md → plan.md → tasks.md → research.md → data-model.md → quickstart.md → contracts/` 순서로 자식 노드가 된다
          - 이 순서에 없는 마크다운 파일이 있으면 목록 끝에 파일명 그대로 추가된다
      L5-D7 DetailStep: `specs/`가 없거나 비어 있다 / 기능 디렉터리가 비어 있다
        Logic(AST):
          - IF 대상이 없음 THEN 빈 트리(또는 자식 없는 노드)로 표시, 오류 없이 계속 동작

L2-B3 Activity: 진행 상태 확인  (3.1~3.4, 4.1~4.5, 5.1~5.5)
  L3-F3a UI: spec-kit 기능 배지
    L4-S3 Step: 기능 노드의 진행 상태를 본다
      L5-D8 DetailStep: `spec.md`/`plan.md`/`tasks.md` 존재 여부로 마일스톤을 판정한다
        Logic(AST):
          - `spec.md` 있음 -> "명세" 마일스톤 완료
          - `plan.md` 있음 -> "설계" 마일스톤 완료
          - `tasks.md` 있음 -> "작업 분해" 마일스톤 완료
          - 아직 없는 파일의 마일스톤은 미완료로 남고, 있는 것만으로 n/총개수가 계산된다
      L5-D9 DetailStep: 세 파일이 모두 있다
        Logic(AST):
          - IF spec.md AND plan.md AND tasks.md THEN 전부 완료 강조 표시(spec-kit 자체에 이 이상의 "구현 검증됨" 표시가 없다는 한계를 그대로 반영)
  L3-F3b UI: `.kiro` 스펙 배지
    L4-S4 Step: 스펙 노드의 진행 상태를 본다
      L5-D10 DetailStep: 기록된 승인 게이트 중 일부만 승인됐다
        Logic(AST):
          - 그 스펙에 실제로 기록된 게이트 개수를 총개수로 삼아 n/총개수 표시(스펙마다 총개수가 달라도 각자 독립적으로 정확함 — `bugfix.md` 전용 경로처럼 게이트가 적은 경우도 그 개수 그대로)
      L5-D11 DetailStep: 기록된 게이트가 모두 승인됐지만 구현 완료 상태는 아니다
        Logic(AST):
          - IF 게이트 전부 승인 AND 구현 완료 후속 상태 아님 THEN "게이트 전부 승인" 표시(구현 완료 표시와는 다른 표시)
      L5-D12 DetailStep: 구현 완료를 나타내는 후속 상태에 도달했다
        Logic(AST):
          - IF 구현 완료 후속 상태 THEN L5-D11과 구분되는 완료 표시
      L5-D13 DetailStep: `spec.json`이 없거나 파싱 실패
        Logic(AST):
          - IF 파싱 실패/부재 THEN 경고 배지 표시, 문서 노드는 정상 표시(기존과 동일)

L2-B4 Activity: 체크박스 진행률 확인  (6.1, 6.2)
  L3-F4 UI: tasks.md 진행률 배지
    L4-S5 Step: tasks.md의 진행 정도를 본다
      L5-D14 DetailStep: 체크박스가 하나 이상 있다
        Logic(AST):
          - IF 체크박스 존재 THEN 완료/전체 개수를 n/m으로 표시, 전부 완료 시 강조 표시(기존 `.kiro` 표시와 동일 형식)
      L5-D15 DetailStep: 체크박스가 하나도 없다
        Logic(AST):
          - IF tasks.md 있음 AND 체크박스 없음 THEN "진행률 없음" 안내(기존과 동일)

L2-B5 Activity: 문서 내용 확인  (7.1, 7.2)
  L3-F5 UI: 문서 패널
    L4-S6 Step: 문서 노드를 선택한다
      L5-D16 DetailStep: 일반 문서(spec.md/plan.md/tasks.md 등)를 선택한다
        Logic(AST):
          - 파일 내용이 마크다운(다이어그램 포함)으로 렌더링된다(기존 파이프라인 그대로)
      L5-D17 DetailStep: `contracts/` 하위 파일을 선택한다
        Logic(AST):
          - 파일 내용이 일반 텍스트/코드 블록 수준으로 표시된다(형식별 특수 렌더링 없음)

L2-B6 Activity: 읽기 전용 신뢰  (8.1)
  L3-F6 UI: (화면 없음 — 개발자는 결과로만 관찰: 파일이 그대로임)
    L4-S7 Step: 뷰어를 아무리 오래 열어 둬도
      L5-D18 DetailStep: 항상
        Logic(AST):
          - spec-kit 루트(`.specify/`, `specs/`) 아래 어떤 파일도 생성·수정·삭제되지 않는다

L2-B7 Activity: 편집모드 자연 연동  (9.1)
  L3-F7 UI: 편집 키
    L4-S8 Step: spec-kit 문서가 표시된 상태에서 편집 키를 누른다
      L5-D19 DetailStep: 항상
        Logic(AST):
          - 추가 구현 없이 기존 편집모드가 그대로 동작해 외부 에디터로 열린다
```

### ✅ 검토 요청 (L1: spec-kit 프로젝트 열람)
승인(✓) 또는 수정 사항을 입력하세요.
