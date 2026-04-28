import { describe, it, expect } from 'vitest'
import { evaluateAssistance } from '@/domain/reducers/policyEvaluator'
import type { AssessmentPolicy, RequestedHelp } from '@/domain/types/policy'

describe('policyEvaluator', () => {
  describe('evaluateAssistance', () => {
    // ─── Practice policy: everything allowed ───
    describe('under Practice policy', () => {
      const policy = 'Practice' as AssessmentPolicy

      const allHelpTypes: RequestedHelp[] = [
        'ExplainConcept',
        'DebugCode',
        'GenerateTests',
        'SolveMultipleChoice',
        'GenerateCompleteCode',
      ]

      it.each(allHelpTypes)('allows %s', (help) => {
        const decision = evaluateAssistance(policy, help)
        expect(decision.allowed).toBe(true)
        expect(decision.requiresConfirmation).toBe(false)
        expect(decision.fallback).toBe('None')
      })
    })

    // ─── PermittedAi policy: everything allowed ───
    describe('under PermittedAi policy', () => {
      const policy = 'PermittedAi' as AssessmentPolicy

      it('allows ExplainConcept', () => {
        const d = evaluateAssistance(policy, 'ExplainConcept')
        expect(d.allowed).toBe(true)
      })

      it('allows DebugCode', () => {
        const d = evaluateAssistance(policy, 'DebugCode')
        expect(d.allowed).toBe(true)
      })

      it('allows GenerateTests', () => {
        const d = evaluateAssistance(policy, 'GenerateTests')
        expect(d.allowed).toBe(true)
      })

      it('allows SolveMultipleChoice', () => {
        const d = evaluateAssistance(policy, 'SolveMultipleChoice')
        expect(d.allowed).toBe(true)
      })

      it('allows GenerateCompleteCode', () => {
        const d = evaluateAssistance(policy, 'GenerateCompleteCode')
        expect(d.allowed).toBe(true)
      })
    })

    // ─── SyntaxOnly policy ───
    describe('under SyntaxOnly policy', () => {
      const policy = 'SyntaxOnly' as AssessmentPolicy

      it('allows ExplainConcept', () => {
        const d = evaluateAssistance(policy, 'ExplainConcept')
        expect(d.allowed).toBe(true)
      })

      it('allows DebugCode', () => {
        const d = evaluateAssistance(policy, 'DebugCode')
        expect(d.allowed).toBe(true)
      })

      it('blocks GenerateTests with SyntaxOnlyGuidance', () => {
        const d = evaluateAssistance(policy, 'GenerateTests')
        expect(d.allowed).toBe(false)
        expect(d.fallback).toBe('SyntaxOnlyGuidance')
      })

      it('blocks SolveMultipleChoice', () => {
        const d = evaluateAssistance(policy, 'SolveMultipleChoice')
        expect(d.allowed).toBe(false)
      })

      it('blocks GenerateCompleteCode', () => {
        const d = evaluateAssistance(policy, 'GenerateCompleteCode')
        expect(d.allowed).toBe(false)
      })
    })

    // ─── RestrictedAssessment policy ───
    describe('under RestrictedAssessment policy', () => {
      const policy = 'RestrictedAssessment' as AssessmentPolicy

      it('allows ExplainConcept', () => {
        const d = evaluateAssistance(policy, 'ExplainConcept')
        expect(d.allowed).toBe(true)
      })

      it('allows DebugCode', () => {
        const d = evaluateAssistance(policy, 'DebugCode')
        expect(d.allowed).toBe(true)
      })

      it('blocks GenerateTests with ConceptualGuidance', () => {
        const d = evaluateAssistance(policy, 'GenerateTests')
        expect(d.allowed).toBe(false)
        expect(d.fallback).toBe('ConceptualGuidance')
      })

      it('blocks SolveMultipleChoice', () => {
        const d = evaluateAssistance(policy, 'SolveMultipleChoice')
        expect(d.allowed).toBe(false)
      })

      it('blocks GenerateCompleteCode', () => {
        const d = evaluateAssistance(policy, 'GenerateCompleteCode')
        expect(d.allowed).toBe(false)
      })
    })

    // ─── UnknownAssessment policy ───
    describe('under UnknownAssessment policy', () => {
      const policy = 'UnknownAssessment' as AssessmentPolicy

      it('requires confirmation for ExplainConcept', () => {
        const d = evaluateAssistance(policy, 'ExplainConcept')
        expect(d.allowed).toBe(false)
        expect(d.requiresConfirmation).toBe(true)
        expect(d.fallback).toBe('AskForPolicyConfirmation')
      })

      it('requires confirmation for DebugCode', () => {
        const d = evaluateAssistance(policy, 'DebugCode')
        expect(d.allowed).toBe(false)
        expect(d.requiresConfirmation).toBe(true)
      })

      it('requires confirmation for GenerateTests', () => {
        const d = evaluateAssistance(policy, 'GenerateTests')
        expect(d.allowed).toBe(false)
        expect(d.requiresConfirmation).toBe(true)
      })

      it('requires confirmation for SolveMultipleChoice', () => {
        const d = evaluateAssistance(policy, 'SolveMultipleChoice')
        expect(d.allowed).toBe(false)
        expect(d.requiresConfirmation).toBe(true)
      })

      it('requires confirmation for GenerateCompleteCode', () => {
        const d = evaluateAssistance(policy, 'GenerateCompleteCode')
        expect(d.allowed).toBe(false)
        expect(d.requiresConfirmation).toBe(true)
      })
    })

    // ─── Decision contains meaningful reason ───
    it('returns a non-empty reason string', () => {
      const d = evaluateAssistance('Practice', 'ExplainConcept')
      expect(d.reason).toBeTruthy()
      expect(typeof d.reason).toBe('string')
      expect(d.reason.length).toBeGreaterThan(0)
    })

    // ─── Blocked decisions do NOT require confirmation ───
    it('blocked decisions never require confirmation', () => {
      const d = evaluateAssistance('SyntaxOnly', 'GenerateCompleteCode')
      expect(d.allowed).toBe(false)
      expect(d.requiresConfirmation).toBe(false)
    })
  })
})
