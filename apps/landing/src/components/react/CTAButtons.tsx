import { Button } from '../ui/Button';

export default function CTAButtons() {
  return (
    <div className="mt-10 flex flex-col sm:flex-row items-center justify-center gap-4">
      <a href="#waitlist">
        <Button variant="primary" size="lg">
          Unirme a la lista
        </Button>
      </a>
      <a href="#waitlist">
        <Button variant="outline" size="lg">
          Solicitar demo enterprise
        </Button>
      </a>
    </div>
  );
}
