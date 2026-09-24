---
inclusion: always
---

# 가치사슬 (Value Chain) — SSoT

> spec-viewer 저장소 자체 부트스트랩(2026-09-24). archgenworks 전체 가치사슬을 끌어오지 않고,
> 이 도구 하나의 가치사슬만 최소 골격으로 새로 정의한다. 기존 `.kiro/specs/spec-viewer/biz-process.md`가
> 이미 참조 중인 `VC-DEV-SPEC-VIEW`와 정확히 일치시켰다(`kiro-biz-process/rules/biz-process-rules.md` §3
> "valueChainRef는 value-chain.md의 Unit.id와 정확히 일치"). 제품/비즈니스 owner가 소유하는 SSoT이며
> `kiro-biz-process` 스킬은 이 파일을 자동 수정하지 않는다.

## Mega (가치사슬 정의서)
- id: VC-DEV
- name: 개발 도구 가치사슬
- value: 개발자가 스펙·문서 작업에 별도 도구 전환 없이 몰입할 수 있다.

## Main (주요 가치 흐름)
- id: VC-DEV-SPEC
- name: 스펙 문서 열람·관리 흐름
- parent: VC-DEV

## Unit (단위 프로세스)

- id: VC-DEV-SPEC-VIEW
  name: 스펙 열람
  parent: VC-DEV-SPEC
  value: 터미널에서 스펙·문서를 읽기 전용으로 빠르게 파악한다.
  validation: 뷰어 오픈부터 종료까지 별도 도구 전환 없이 트리 탐색·문서 내 이동·검색·진행률 확인이 가능하다.
  bizProcessRef: BP-SPEC-VIEW

- id: VC-DEV-SPEC-EDIT
  name: 문서 즉시 편집
  parent: VC-DEV-SPEC
  value: 뷰어에서 발견한 오탈자·구조 문제를 그 자리에서 바로 고친다.
  validation: 편집 키 입력부터 에디터 복귀·재렌더까지 별도 터미널 전환 없이 완료된다.
  bizProcessRef: BP-SPEC-EDIT
