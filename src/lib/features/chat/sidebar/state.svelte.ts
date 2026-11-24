import { jobKind } from '../jobs'

export type SessionSidebarView = 'jobs' | 'agents' | 'toolbox' | 'context' | 'files' | 'plan' | 'memory'

export class SessionSidebarState {
  view = $state<SessionSidebarView | null>(null)
  selectedJob = $state<string | null>(null)
  selectedAgent = $state<string | null>(null)

  toggle(next: SessionSidebarView): void {
    this.view = this.view === next ? null : next
  }

  openJob(ref?: string): void {
    if (!ref) {
      this.view = 'jobs'
    } else if (jobKind(ref) === 'command') {
      this.view = 'jobs'
      this.selectedJob = ref
    } else {
      this.view = 'agents'
      this.selectedAgent = ref
    }
  }
}
