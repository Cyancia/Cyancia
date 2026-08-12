use wgpu::{Device, PollError, PollStatus, PollType, SubmissionIndex};

pub trait DevicePollExt {
    fn poll_indefinitely_for(
        &self,
        submission_index: SubmissionIndex,
    ) -> Result<PollStatus, PollError>;
}

impl DevicePollExt for Device {
    fn poll_indefinitely_for(
        &self,
        submission_index: SubmissionIndex,
    ) -> Result<PollStatus, PollError> {
        self.poll(PollType::Wait {
            submission_index: Some(submission_index),
            timeout: None,
        })
    }
}
