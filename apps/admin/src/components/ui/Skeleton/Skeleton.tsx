import { motion } from "framer-motion";
import { cn } from '../../../lib/utils';;

function Skeleton({
    className,
    ...props
}: React.HTMLAttributes<HTMLDivElement>) {
    return (
        <div
            className={cn(
                "relative overflow-hidden rounded-[5px] bg-white/[0.05]",
                className
            )}
            {...props}
        >
            <motion.div
                className="absolute inset-0 bg-gradient-to-r from-transparent via-white/[0.08] to-transparent"
                initial={{ translateX: "-100%" }}
                animate={{ translateX: "100%" }}
                transition={{
                    repeat: Infinity,
                    duration: 1.5,
                    ease: "linear",
                }}
            />
        </div>
    );
}

export { Skeleton };
