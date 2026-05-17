import { Button } from '../ui/Button';

export default function CTAButtons() {
  return (
    <div className="mt-10 flex flex-col sm:flex-row items-center justify-center gap-4">
      <a href="#waitlist">
        <Button variant="primary" size="lg">
          Join the Waitlist
        </Button>
      </a>
      <a href="#waitlist">
        <Button variant="outline" size="lg">
          Request Enterprise Demo
        </Button>
      </a>
    </div>
  );
}
